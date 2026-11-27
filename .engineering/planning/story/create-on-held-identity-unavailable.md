---
format: aep.planning-md/3
id: story:create-on-held-identity-unavailable
kind: story
status: draft
title: A create on a held identity is refused as unavailable
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-cli/src/replay.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/adversary_w26_u4.rs
- confidence: cited
  path: crates/spindle-conformance/src/target.rs
- confidence: cited
  path: crates/spindle-conformance/tests/spindle-conformance/adversary_establish.rs
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/execute/affects.rs
- confidence: inferred
  path: crates/spindle-kernel/src/lib.rs
- confidence: cited
  path: crates/spindle-telemetry/tests/spindle-telemetry/catalogue.rs
revision: 10
---
# A create on a held identity is refused as unavailable

## Outcome

A creating command whose identity is already held (by an establish step or an earlier create)
answers as ESS's interpreter answers it: unavailable, because creation never replaces a row. Today
the kernel answers `Undeclared` (`crates/spindle-kernel/src/execute.rs`), which the conformance
target maps to an undeclared result (`crates/spindle-conformance/src/target.rs`).

## Acceptance

- `adversary_establish.rs::a_create_on_an_established_identity_answers_as_the_interpreter` runs
  (its `#[ignore]` is removed) and passes.
- `a_second_create_on_one_identity_answers_as_the_interpreter` — two creates, no establish step.
- `a_replay_that_diverges_on_a_held_identity_prints_unavailable`: `spindle replay` on a scenario
  whose create meets a held identity and whose expected answer differs prints the divergent line
  ending `, unavailable`.

## Origin

Pre-existing. Found by the adversary of `story:scenario-establish-steps` in wave 7.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel` (the held-identity refusal) and
  `crates/spindle-conformance` (its answer to the runner) — cited
- **Files:** `crates/spindle-kernel/src/execute.rs:2076-2136` (`creation_identity`; the refusal of a
  supplied identity already held at `:2107-2114`) — cited
- **Files:** `crates/spindle-kernel/src/execute.rs:826` (`Run::select`) and `:1837`
  (`Run::wrong_state`), `crates/spindle-kernel/src/execute/affects.rs:240` and the test
  `crates/spindle-telemetry/tests/spindle-telemetry/catalogue.rs:186-189`: the other
  constructors of `ExecuteError::Undeclared`, which gain the new field as `None` (one line each);
  `:826` sits in the hunk U4 rewrites, so U5 merges after U4 — cited (`git grep -n "Undeclared *{"`)
- **Files:** `crates/spindle-conformance/src/target.rs:377-378` (`ExecuteError::Undeclared` mapped to
  `SemanticCommandResult::undeclared()`), and `Answered` at `:79-105` — cited
- **Files:** `crates/spindle-conformance/tests/spindle-conformance/adversary_establish.rs:174-205`
  (remove the `#[ignore]` from `a_create_on_an_established_identity_answers_as_the_interpreter`; add
  `a_second_create_on_one_identity_answers_as_the_interpreter`) — cited
- **Files:** `crates/spindle-cli/tests/spindle-cli/adversary_w26_u4.rs:532` (expects
  `diverges, undeclared` for a second create of one identity; that line changes) — cited
- **Symbols:** `ExecuteError::Undeclared`, `Selection::creation_identity`,
  `HostTarget::execute_command`, `Answered::of` — cited
- **Also likely:** `crates/spindle-kernel/src/lib.rs:225-232` (the held cause on
  `ExecuteError::Undeclared`, D1) — inferred
- **Also likely:** `crates/spindle-kernel/src/execute/affects.rs:239-243` and
  `crates/spindle-telemetry/tests/spindle-telemetry/catalogue.rs:186-189` (they build
  `Undeclared { command, reason }` and gain the new field) — inferred
- **Also likely:** `crates/spindle-cli/src/replay.rs:239-253` (a replay prints the refusal on the
  divergent line, D3 option B) — inferred
- **Documents:** none
- **Confidence:** high — the story names both defect sites, the tree confirms them, and the
  interpreter's answer is read from the pinned source
- **Would collide with:** any unit that changes `spindle-kernel`'s `execute.rs` or the
  `ExecuteError` enum in `lib.rs`, or `spindle-conformance`'s `target.rs` command mapping
- **Safety fact:** the kernel's refusal message (`Display`) and `ExecuteError::kind()` stay as they
  are, so the CLI's `failure.kind`/`message` output (`spindle-cli/src/lines.rs:274-277`), the
  telemetry label set (`spindle-telemetry/src/port.rs:104-116`) and the recorded wasm answer
  (`spindle-wasm/tests/fixtures/review/exchanges.json:18`) do not move. Only the conformance target's
  answer changes, and `explore.rs:315-329` already treats an `unavailable` whose detail contains "the
  request is not what the model declares" as matching the kernel's undeclared answer — level 2,
  unproven

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: `ExecuteError::Undeclared` gains a held-identity field; `kind()` and the message stay.
- D2 A: the conformance target answers with ESS's operation and detail word for word.
- D3 B: an `Answered` variant for an unavailable answer, printed as `, unavailable` on the
  replay's divergent line.

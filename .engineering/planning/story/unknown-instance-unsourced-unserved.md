---
format: aep.planning-md/3
id: story:unknown-instance-unsourced-unserved
kind: story
status: draft
title: An unknown-instance refusal with unsourced fields is not served
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: README.md
- confidence: cited
  path: crates/spindle-capability/src/mint.rs
- confidence: cited
  path: crates/spindle-capability/tests/spindle-capability/error_fields.rs
- confidence: cited
  path: crates/spindle-host/tests/fixtures/adversary/notice/sink.yaml
- confidence: inferred
  path: crates/spindle-ingress/tests/fixtures/unknown-refusal
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/main.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/unknown_refusal.rs
- confidence: inferred
  path: crates/spindle-kernel/src/execute.rs
- confidence: inferred
  path: crates/spindle-kernel/src/mint.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/error_fields.rs
revision: 5
---
# An unknown-instance refusal with unsourced fields is not served

## Outcome

A command whose explicit `unknown_instance:` branch answers an error with a field the specification
gives no source is treated as ESS 0.87.0's served application treats it: it does not serve that
answer (`ess-synth` `determined.rs`, "the fields of error ... which the specification gives no
source"). Today spindle answers such a refusal with the field left out, a body the published schema
refuses (`ess-gen` `openapi.rs:2519-2575` requires the full payload on that branch).

## Acceptance

- `an_unknown_instance_refusal_with_an_unsourced_field_is_not_served`: the route (or, with
  `story:outcome-scoped-refusal`, that outcome) answers 501 naming the field; nothing is written.
- `an_unknown_instance_refusal_whose_fields_all_have_sources_is_served`: answers the full payload.

## Origin

Correction round of `story:refusal-payload-complete`, wave 10, 2026-11-06.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-capability` (the rule for unsourced error fields) and `crates/spindle-kernel` (refusing the outcome) — cited
- **Files:** `crates/spindle-capability/src/mint.rs:184-256` — cited, `unsourced` gives every unsourced field of an `unknown_instance:` refusal `Unsourced::Unanswered` (line 248)
- **Files:** `crates/spindle-kernel/src/supported.rs:237,508-521` — cited, `unfilled_error_field` refuses an outcome by name only for `Unsourced::Unfilled` today; with D1 A it also refuses a required `Unanswered` field (wave 29 also changes this file)
- **Files:** `crates/spindle-kernel/tests/spindle-kernel/error_fields.rs:145-182` — cited, `a_default_refusal_of_a_subject_guarded_command_reads_its_field_from_the_row` asserts that `no-such-book` answers an empty payload; under D1 A it gets `ExecuteError::Unsupported` naming `study.loan.NoSuchBook.book_id`
- **Files:** `crates/spindle-capability/tests/spindle-capability/error_fields.rs:35-47` — cited, the assertion still holds (no mint), only its message text describes the old rule
- **Files:** `crates/spindle-host/tests/fixtures/adversary/notice/sink.yaml:44-47` — cited, `no-board` leaves the required `NoBoard.board_id` unsourced; it serves `adversary_retry.rs` (host and conformance), `refusal.rs` and `spindle-cli` `adversary_w26_u4.rs`
- **Symbols:** `spindle_capability::mint::unsourced`, `Unsourced::Unanswered`, `spindle_kernel::supported::unfilled_error_field` — cited
- **Also likely:** `crates/spindle-kernel/src/mint.rs:363-374` — inferred, a message for this case next to `unfilled` (the current text says "whose type the runtime does not mint", which would be wrong here)
- **Also likely:** `crates/spindle-ingress/tests/spindle-ingress/unknown_refusal.rs` (new), `crates/spindle-ingress/tests/fixtures/unknown-refusal` (new), `crates/spindle-ingress/tests/spindle-ingress/main.rs` (one `mod` line) — inferred, the two acceptance tests on the in-process `Room` harness that `refusal_payload.rs` uses: `an_unknown_instance_refusal_with_an_unsourced_field_is_not_served` (501 naming the field, nothing written, other outcomes served) and `an_unknown_instance_refusal_whose_fields_all_have_sources_is_served` (full payload, checked against `ess_gen::openapi`); each in-process, far under 60 s; the test step stays within 120 s
- **Also likely:** `crates/spindle-kernel/src/execute.rs:2352-2381` — inferred, doc comment only; the `Unanswered => None` arm stays valid because `runs` refuses the outcome before it is reached
- **Documents:** `README.md:167` — inferred, the capability table describes how unsourced error fields are filled
- **Confidence:** high — the defect site, the hook that refuses by name and the two tests that pin today's behaviour are all read from the tree
- **Would collide with:** spindle-kernel `supported.rs` (wave 29: conversions and value sources); the kernel's error-payload path in `execute.rs` and `tests/spindle-kernel/error_fields.rs` (only if a doc comment is touched); `crates/spindle-ingress/tests/spindle-ingress/main.rs`; `README.md`
- **Safety fact:** the change reaches only outcomes with `ResolvedCondition::UnknownInstance` that leave a required error field unsourced; in the tree that is exactly `held-refusal` (`NoSuchBook.book_id`) and the host `notice` sink (`NoBoard.board_id`); every other `unknown_instance:` error in the YAML fixtures and `examples/` declares `fields: []` or sources every field — step 2 (git grep of `unknown_instance:` plus each error's `fields:`), unproven

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: a required unsourced field of an `unknown_instance:` refusal makes that outcome
  refuse by name (501 when a request selects it, listed by `admit`); an `Optional` one stays
  absent.
- D2 A: the host fixture `adversary/notice/sink.yaml` sources `NoBoard.board_id` from the
  input.

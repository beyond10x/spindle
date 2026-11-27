---
format: aep.planning-md/3
id: story:error-payload-generated-fields
kind: story
status: implemented
title: A generated field in an error payload is minted as ESS mints it
relations:
- serves: vision:spindle
- depends_on: story:unknown-instance-unsourced-unserved
scope:
- confidence: inferred
  path: crates/spindle-capability/src/mint.rs
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/mint.rs
- confidence: cited
  path: crates/spindle-kernel/src/values.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/fixtures/generated_error
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/generated.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/mint.rs
revision: 14
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T09:51:59Z", actor: "human:operator", revision: 12, decided_on: {"recorded":{"review_outcome":8}}}
- {from: "proposed", to: "active", at: "2026-11-27T09:51:59Z", actor: "human:operator", revision: 13, decided_on: {"recorded":{"review_outcome":8}}}
- {from: "active", to: "implemented", at: "2026-11-27T17:04:22Z", actor: "human:operator", revision: 14, decided_on: {"recorded":{"test_result":1,"review_outcome":9}}}
---
# A generated field in an error payload is minted as ESS mints it

## Outcome

The kernel mints `{generated: true}` in a declared error's payload (`a_generated_error_field_is_minted`, wave 3). ESS 0.99.1's interpreter mints none there (`declared_error`), so it draws no tick. Under a mint that answers ESS's tick UUIDs, every draw after such a refusal runs one tick ahead of ESS. Found while fixing the draw order for generated Booleans (wave 5). Pre-existing.

## Acceptance

- `an_error_payload_draws_as_ess_draws`: a command refused with a declared error whose payload
  marks a field generated, followed by a creation that mints: the minted values equal ESS's
  interpreter for the same sequence under the tick mint.
- `an_unsourced_error_field_draws_as_ess_draws`: the same with an error field that has no source.
- `a_seeded_mint_answers_error_values_from_its_seed`: under a seeded mint, the identities minted
  after a refused command equal those of the same run without the refusal.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel` — cited
- **Files:** `crates/spindle-kernel/src/mint.rs` (`Generator::generated`, `Generator::choose`, `Mint`, `Seeded`, module doc line 19 "in `sets:`, a payload or an error") — cited
- **Files:** `crates/spindle-kernel/src/values.rs:433` (`Sources::generated`, reached with `error: true`) — cited
- **Files:** `crates/spindle-kernel/src/execute.rs:2318-2420` (`declared_error`, `error_of`, `unsourced_error_fields`) — cited
- **Files:** `crates/spindle-kernel/tests/spindle-kernel/generated.rs` (`a_generated_error_field_is_minted`:300; new `an_error_payload_draws_as_ess_draws`, compared with `Interpreted::for_model` like the lamp cases) — cited
- **Symbols:** `Generator`, `Mint`, `Seeded`, `Sources::generated`, `Executor::unsourced_error_fields`, `Unsourced::Minted` — cited
- **Parity target:** ESS 0.99.1 `ess-conformance/src/interpret/execute.rs:1948` `declared_error`, `error_value`: in the scenario path (`operation: None`, `responding`:525) a `{generated: true}` error field and an unsourced one are not carried and draw no tick; the interpreter is unchanged at 0.99.2 — cited
- **Also likely:** `crates/spindle-kernel/tests/fixtures/generated_error/` — inferred, a new fixture: one refusal with a `{generated: true}` `Uuid` error field and one unsourced field, then a creation that mints
- **Also likely:** `crates/spindle-kernel/tests/spindle-kernel/mint.rs` — inferred, `a_seeded_mint_answers_error_values_from_its_seed` under D1 A
- **Also likely:** `crates/spindle-capability/src/mint.rs:209-222` — inferred, doc of `unsourced` only; the rule does not change
- **Documents:** none
- **Tests to add:** `an_error_payload_draws_as_ess_draws` (refusal with a generated error field, then a creation under a tick mint, equal to the interpreter); `an_unsourced_error_field_draws_as_ess_draws` (D2 A); `a_seeded_mint_answers_error_values_from_its_seed` (D1 A). Each runs in-process on a small fixture, expected well under 60 s; the step budget of 120 s is not moved — inferred, not timed
- **Confidence:** high — the tree and the ESS source both show where the draw happens; the mint design is D1
- **Would collide with:** the declared-error path of `spindle-kernel` (`execute.rs` `error_of`/`unsourced_error_fields`), `values.rs` `Sources`, and the `Mint`/`Generator` surface of `mint.rs`
- **Safety fact:** a refusal writes no row and emits no event, so a value not drawn from the shared `Mint` is seen only in the returned `DeclaredError` (HTTP payload `spindle-ingress/src/answer.rs:1001`, host refusal record, conformance target `spindle-conformance/src/target.rs:367`, which ESS's interpreted side leaves empty) — step 2, unproven

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: `Mint` gains a method with a default that answers a fresh v4 UUID and draws no
  tick; `Seeded` answers it from a second stream of its seed; every field of a declared error uses
  it.
- D2 A: the unsourced error fields get the same treatment.

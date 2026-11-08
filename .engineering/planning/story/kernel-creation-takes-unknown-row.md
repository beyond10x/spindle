---
format: aep.planning-md/3
id: story:kernel-creation-takes-unknown-row
kind: story
status: implemented
title: A creation takes an absent row beside unknown_instance as ESS does
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: inferred
  path: crates/spindle-kernel/src/execute/delete.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/creation_takes_unknown_row.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/main.rs
revision: 8
transitions:
- {from: "draft", to: "proposed", at: "2026-11-07T18:59:08Z", actor: "human:timo", revision: 6}
- {from: "proposed", to: "active", at: "2026-11-07T18:59:09Z", actor: "human:timo", revision: 7}
- {from: "active", to: "implemented", at: "2026-11-08T17:25:49Z", actor: "human:timo", revision: 8, decided_on: {"recorded":{"test_result":1,"review_outcome":1,"verification":1}}}
---
# A creation takes an absent row beside `unknown_instance:` as ESS does

## Outcome

Take a command with no row set that declares `unknown_instance:` and has a creation branch taking
its identity from the same input field. It answers an absent addressed row as ESS 0.90.0's
interpreter does (`addressed_unknown`, `ess-conformance` `src/interpret/execute.rs` line 1497 at
rev 9a2468e). The creation may take the row, and the `unknown_instance:` answer applies only where
no creation is selected. Where the command declares no `unknown_instance:` and ESS falls back to its
one declared not-found refusal (`declared_not_found`, the same file's `unknown_instance` at line
2460), the kernel answers the same.

Today, outside row sets, the kernel answers every absent addressed row at its existence step before
any creation is considered. The existence step is `Phase::Existence` in `Run::select`
(`crates/spindle-kernel/src/execute.rs`, line 447), and `Run::look_ahead` (line 936) reads the same
step.

A delete is not affected. `Run::delete` (`crates/spindle-kernel/src/execute/delete.rs`) calls
`Run::unknown_instance` at line 26 for an absent row. A delete outcome never creates, so no creation
can be selected beside it, and its answer stays as it is.

## Acceptance

The fixture is a neutral one in the spindle-kernel test binary, read on both sides against ESS's
interpreter at 0.90.0. In each case below, the test equal to the interpreter is
`addressed_unknown_agrees_with_the_interpreter`. It compares outcomes, errors, payloads and stored
rows step by step, in the style of `adversary_related_values.rs` line 271.

- `a_creation_takes_an_absent_row_beside_unknown_instance`: steps (1) `Open` of an absent identity
  whose creation guard holds. The creation branch is selected, and the row exists afterwards in its
  initial state.
- `an_absent_row_no_creation_takes_answers_unknown_instance`: steps (1) `Open` of an absent identity
  whose creation guard fails. The `unknown_instance:` branch answers, and no row is written.
- `an_absent_row_without_unknown_instance_answers_the_declared_not_found`: a second command of the
  fixture with no `unknown_instance:` and one declared not-found refusal. Steps (1) that command on
  an absent identity whose creation guard fails. The declared not-found refusal answers, and no row
  is written.
- `addressed_unknown_agrees_with_the_interpreter`: the three step lists above, each run on a fresh
  store, equal ESS's interpreter in outcome, error, payload and rows.

## Scope

- `crates/spindle-kernel/src/execute.rs`: cited. The existence step (`Phase::Existence` in
  `Run::select`, line 447), `Run::look_ahead` (line 936), `Run::unknown_instance` (line 1088) and
  `Run::take` (line 1160).
- `crates/spindle-kernel/src/execute/delete.rs`: inferred. It calls `Run::unknown_instance` at line
  26. Its behaviour does not change, but it moves if that function's signature does.
- `crates/spindle-kernel/tests/spindle-kernel/creation_takes_unknown_row.rs`: inferred, new, with
  its fixture.
- `crates/spindle-kernel/tests/spindle-kernel/main.rs`: inferred. The
  `mod creation_takes_unknown_row;` line.

## Order

- `crates/spindle-kernel/src/execute.rs` is shared with `story:kernel-secondary-effects`,
  `story:kernel-set-outcomes`, `story:state-change-conditions`,
  `story:response-field-value-source`, `story:kernel-guard-reads-now`,
  `story:kernel-row-set-commit-conflict` and `story:error-payload-generated-fields`.
- `crates/spindle-kernel/tests/spindle-kernel/main.rs` is shared with `story:kernel-guard-reads-now`,
  which adds a `mod` line too.

## Origin

Pre-existing. It was found while fixing the row-set upsert precedence in `story:kernel-row-set-guards`
(wave 14), which made the same rule hold beside a row set only.

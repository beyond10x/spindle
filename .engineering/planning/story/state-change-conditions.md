---
format: aep.planning-md/3
id: story:state-change-conditions
kind: story
status: implemented
title: State-change conditions
relations:
- serves: vision:spindle
- decomposes: epic:m0-run-in-process
scope:
- confidence: inferred
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/state_change.rs
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T21:50:11Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T21:50:11Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-11T23:08:11Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":1}}}
---
# State-change conditions

## Outcome

The kernel selects an outcome that declares `when_state_changes: true` or `false`: the condition
holds when the state the outcome would move the subject to differs (`true`) or does not differ
(`false`) from the state it holds immediately before selection, conjunctive with the outcome's
`when:`. Today `crates/spindle-kernel/src/supported.rs` refuses `ResolvedCondition::StateChange`.

Requested by a downstream specification probe (2026-11-04): 11 of its commands are refused on
this construct. Reproduced with a neutral fixture.

## Acceptance

- `the_first_report_of_a_state_takes_the_changing_branch` — a subject not yet in the target
  state takes the `when_state_changes: true` branch and moves.
- `a_repeated_report_falls_to_the_unchanged_branch` — the same input again takes the other branch
  and leaves the state as it was.
- `state_change_reads_the_state_before_selection` — after a restart on SQLite the held state read
  is the persisted one.

## Scope

- `crates/spindle-kernel/src/supported.rs` — cited: `StateChange` in `condition`.
- `crates/spindle-kernel/src/execute.rs` — inferred: outcome selection.
- `crates/spindle-kernel/tests/state_change.rs` — inferred, new.

ESS semantics: `docs/design/state-change-outcome-guards.md` at ess 0.86.0.

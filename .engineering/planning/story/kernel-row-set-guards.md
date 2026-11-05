---
format: aep.planning-md/3
id: story:kernel-row-set-guards
kind: story
status: draft
title: Guards over a row set
relations:
- serves: vision:spindle
- decomposes: epic:downstream-kernel-constructs
scope:
- confidence: inferred
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/store.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/row_set_guards.rs
revision: 5
---
# Guards over a row set

## Outcome

The kernel selects an outcome whose condition is `when_related:` over a row set: every row of the
selection's entity is a candidate, a row is selected where the `where:` predicate holds of it
(over its fields, identity and held state, `input.`, `subject.` and the decision's `now`), and the
test (`exists: true` / `false`) is decided over the selected rows, all read from the store as it
was before selection. Today `crates/spindle-kernel/src/supported.rs` (`condition`) refuses
`ResolvedCondition::RelatedSet` as "`when_related:` over a row set".

Requested by a downstream specification probe (2026-11-05, 44 commands): 3 commands are refused
first on this construct; their selections use `all:` conjunctions, state tests and an `exists`
quantifier over an input list. Reproduced with a neutral fixture.

ESS semantics: ESS 0.86.0's interpreter, `crates/verify/ess-conformance/src/interpret/execute/row_set.rs`
(`Reader::decide`, `decisions`).

## Acceptance

Fixture: a library whose `Reserve` command is refused with `already-reserved` when a reservation
row with the same `reader_id` and `book_id` in state `Active` exists, and with `unknown-shelfmark`
when no catalogue row's code is among the input list `shelfmarks`.

- `an_existing_matching_row_takes_the_row_set_branch` — an active reservation refuses the
  second one; nothing is written.
- `a_row_in_another_state_does_not_match` — a cancelled reservation does not refuse.
- `a_quantifier_over_an_input_list_selects_rows` — `exists: {in: input.shelfmarks, …}` matches.
- `row_set_guards_agree_with_the_interpreter` — for the same steps, outcomes, events and rows
  equal what ESS 0.86.0's interpreter produces.

## Scope

- `crates/spindle-kernel/src/supported.rs` — cited: `RelatedSet` in `condition`.
- `crates/spindle-kernel/src/execute.rs` — inferred: outcome selection (`holds`).
- `crates/spindle-kernel/src/store.rs` — cited: `Store::scan`.
- `crates/spindle-kernel/tests/row_set_guards.rs` — inferred, new.

## Depends on

Nothing. Shares the selector over `Store::scan` with `story:kernel-set-outcomes`.

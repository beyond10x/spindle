---
format: aep.planning-md/3
id: story:kernel-related-row-guards
kind: story
status: draft
title: Guards over one related row
relations:
- serves: vision:spindle
- decomposes: epic:downstream-kernel-constructs
scope:
- confidence: inferred
  path: crates/spindle-kernel/src/execute.rs
- confidence: inferred
  path: crates/spindle-kernel/src/facts.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/related_guards.rs
revision: 5
---
# Guards over one related row

## Outcome

The kernel selects an outcome whose condition is `when_related:` over one row: the row of the
named entity whose identity an input field (or, from ess/22, a stored field of the addressed
subject) names, read as the store held it before selection. `exists: false` holds where the input
names an identity no row carries; a test over the row's fields or state holds as evaluated on that
row. An absent `Optional` reference reads no row and selects no related branch. Today
`crates/spindle-kernel/src/supported.rs` (`condition`) refuses `ResolvedCondition::Related` as
"`when_related:` over one row".

Requested by a downstream specification probe (2026-11-05, 44 commands): 3 commands are refused
first on this construct and 2 more declare it behind another refusal. Reproduced with a neutral
fixture.

ESS semantics: ESS 0.86.0's interpreter, `crates/verify/ess-conformance/src/interpret/execute/related.rs`
(`missing`, `reference`, `held`, `absent_optional`).

## Acceptance

Fixture: a warehouse whose `PickItem` command names a shelf by `input.shelf_id`, with a
`no-shelf` refusal on `exists: false` and a `shelf-closed` refusal on the shelf's state `Closed`.

- `a_missing_related_row_takes_the_exists_false_branch` — a shelf id no row carries is refused
  with `no-shelf`; nothing is written.
- `a_test_on_the_related_row_selects_its_branch` — a closed shelf is refused with
  `shelf-closed`; an open one is picked.
- `an_absent_optional_reference_selects_no_related_branch`.
- `a_reference_stored_on_the_subject_is_followed` — the row named by the addressed subject's
  stored field is the one read.
- `related_row_guards_agree_with_the_interpreter` — for the same steps, outcomes, events and rows
  equal what ESS 0.86.0's interpreter produces.

## Scope

- `crates/spindle-kernel/src/supported.rs` — cited: `Related` in `condition`.
- `crates/spindle-kernel/src/execute.rs` — inferred: outcome selection (`holds`).
- `crates/spindle-kernel/src/facts.rs` — inferred: the related row as guard facts.
- `crates/spindle-kernel/tests/related_guards.rs` — inferred, new.

## Depends on

Nothing.

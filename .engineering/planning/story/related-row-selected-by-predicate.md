---
format: aep.planning-md/3
id: story:related-row-selected-by-predicate
kind: story
status: implemented
title: A value source reads the one row a predicate selects
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-24T02:29:36Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-11-24T02:29:36Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-11-24T02:29:37Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
# A value source reads the one row a predicate selects

## Outcome

`{related: {entity, where, field}}` (ess/22) in `sets:` or an event payload reads `field` of the
one row of `entity` whose stored fields satisfy `where`, as the store held it before the outcome,
as ESS's interpreter reads it (`interpret::execute::row_set::Reader::one`,
`determined::selected`). Today the kernel refuses every outcome that carries one, so a
specification that links a request to a stored row by a field other than its identity does not
run.

## Acceptance

- `a_value_reads_the_one_row_a_predicate_selects` — an outcome whose value source selects a row
  by a stored field equal to an input field runs: the event and the written row carry the
  selected row's value, equal to ESS's interpreter for the same steps; no matching row or more
  than one gives no value and writes nothing.

## Scope

- `crates/spindle-kernel/src/supported.rs` — the refusal.
- `crates/spindle-kernel/src/values.rs`, `crates/spindle-kernel/src/execute.rs` — the read.
- `crates/spindle-kernel/tests/spindle-kernel/` — the tests, with a neutral fixture.

---
format: aep.planning-md/3
id: story:value-source-through-two-rows
kind: story
status: draft
title: A value source read through two related rows runs
relations:
- serves: vision:spindle
- informed_by: story:related-row-selected-by-predicate
revision: 1
---
## Outcome

A value source that reads a field of a row reached through another row (input id → row A's foreign
key → row B's field) runs in `run` and `conform`, as ESS 0.99.1's interpreter reads it. Today it is
refused with "unsupported: the value source <B.field> of the row <A.fk> of the row input.<id>
names" (`crates/spindle-kernel/src/supported.rs:928`).

## Why

Reported by a consumer on 2026-11-27 against 0.22.0: the outcome that creates an instance reads a
list field of a formation through the workload the input names, and its walking skeleton needs it.
`story:kernel-related-row-values` and `story:related-row-selected-by-predicate` read one related
row, not a chain of two.

## Acceptance

- `a_value_source_through_two_rows_reads_the_far_row`: a neutral fixture with three entities, where
  an outcome copies a field of the row two hops from the input; the command answers, the row holds
  the far row's value, and ESS's interpreter gives the same row.
- `a_missing_middle_row_is_refused_as_ess_refuses_it`: with the middle row absent, the command is
  answered as ESS's interpreter answers it.

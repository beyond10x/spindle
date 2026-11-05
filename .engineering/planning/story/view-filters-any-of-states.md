---
format: aep.planning-md/3
id: story:view-filters-any-of-states
kind: story
status: active
title: View filters over several states
relations:
- serves: vision:spindle
- decomposes: epic:m0-run-in-process
scope:
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-kernel/src/store.rs
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T21:50:13Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T21:50:13Z", actor: "human:timo", revision: 3}
---
# View filters over several states

## Outcome

A view whose filter is `any:` of lifecycle-state comparisons (`any: [state == A, state == B]`) is
answered: the rows are the instances in any of the named states. Today the host's list read takes
only one `state == <State>`, so such a view is not answerable.

Requested by a downstream specification probe (2026-11-04): one of its views is refused on this
filter. Reproduced with a neutral fixture.

## Acceptance

- `a_view_over_two_states_lists_both` — instances in either state are rows; one in a third state
  is not.
- `an_any_filter_mixing_state_and_field_comparisons_is_answered_or_refused_by_name` — a filter
  shape the host does not answer is refused naming the filter, never answered with a subset.

## Scope

- `crates/spindle-host/src/host.rs` — cited: the list read.
- `crates/spindle-kernel/src/store.rs` — inferred: `Store::scan`.
- `crates/spindle-host/tests/` — inferred, new test.

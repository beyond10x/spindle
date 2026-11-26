---
format: aep.planning-md/3
id: story:view-filter-map-value-order
kind: story
status: draft
title: A view filter orders a Map value by its declared type
relations:
- serves: vision:spindle
- informed_by: story:map-input-guard-facts
revision: 1
---
## Outcome

A view filter whose quantifier reads a value inside a `Map` (`Map<String, Timestamp>`, a newtype
over `Timestamp` as the value) orders it by its declared type, as ESS 0.99.1's interpreter does.
`crates/spindle-host/src/view.rs:189` and `:688` type a filter path with `resolve_path` only, which
handles a list position and not a map one, so such a value may compare as text by its bytes.

Found by the wave 27 U4 implementor while fixing the same gap in the kernel's `Facts::declared`;
not yet reproduced.

## Acceptance

- `a_view_filter_orders_a_map_value_by_its_instant`: a view filter with `exists` over a
  `Map<String, Timestamp>` field answers the rows ESS's interpreter answers, for an instant written
  with an offset that sorts differently as text.

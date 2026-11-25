---
format: aep.planning-md/3
id: review-result:view-field-wire-names-design-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on the view wire names story
relations:
- reviews: story:view-field-wire-names
revision: 1
---
needs-revision

- story:view-field-wire-names — it and `story:wire-names-everywhere` both claim a view's rows under wire names, and no relation or sentence records the carve-out.
- story:view-field-wire-names — its acceptance expects an absent `Optional` left out of the row, which `story:view-rows-absent-optional-fields` changes in `crates/spindle-host/src/view.rs`; no edge or sentence records the order.

No cycle; the one edge is `serves vision:spindle`; `wire.rs` comes from the implemented `story:http-outcome-bodies`.

```findings
- file: .engineering/planning/story/wire-names-everywhere.md
  line: 22
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "story:view-field-wire-names and story:wire-names-everywhere both claim a view's rows under wire names and no relation or sentence records the carve-out"
- file: .engineering/planning/story/view-field-wire-names.md
  line: 69
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the acceptance expects an absent Optional left out of the row, but story:view-rows-absent-optional-fields changes that row state and no edge or sentence records the order"
```

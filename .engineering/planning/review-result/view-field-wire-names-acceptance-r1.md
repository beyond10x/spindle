---
format: aep.planning-md/3
id: review-result:view-field-wire-names-acceptance-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on the view wire names story
relations:
- reviews: story:view-field-wire-names
revision: 1
---
needs-revision

- story:view-field-wire-names — the Outcome promises values written by type (struct members under wire names, enum variants in wire spelling), and no fixture field or scenario covers a struct or enum view field.
- story:view-field-wire-names — `a_view_row_is_answered_under_wire_names` joins two independent outcomes (the body admitted by the projected schema, and the rows equal to the renamed interpreter rows).

The other statements name an observable outcome and their transition; the parameter scenario is a guard that holds today and after.

```findings
- file: .engineering/planning/story/view-field-wire-names.md
  line: 60
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome promises values written by type (struct members under wire names, enum variants in wire spelling) but no acceptance scenario or fixture field covers a struct or enum view field"
- file: .engineering/planning/story/view-field-wire-names.md
  line: 68
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "a_view_row_is_answered_under_wire_names joins two independent outcomes (body admitted by the projected schema, and rows equal to the renamed interpreter rows)"
```

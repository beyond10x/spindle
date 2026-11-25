---
format: aep.planning-md/3
id: review-result:view-filter-always-and-key-acceptance-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on the always and key filter story
relations:
- reviews: story:view-filter-always-and-key
revision: 1
---
needs-revision

- story:view-filter-always-and-key — the Outcome says any other leaf and any other two-segment path is still refused whole, but no acceptance scenario submits one and checks the refusal, so the boundary can widen without any scenario failing.
- story:view-filter-always-and-key — the Outcome admits a key path compared with another field, listed and `defined`, but the scenarios compare only against a parameter or a literal.

The before-state (each view refused by `spindle_host::reads`, the route 501), the observable outcomes (rows equal to ESS's interpreter's) and the transition (the reader admitting `Always`/`Never` and the struct-member path) hold.

```findings
- file: .engineering/planning/story/view-filter-always-and-key.md
  line: 44
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome says any other leaf and any other two-segment path is still refused whole, but no acceptance scenario submits one and checks the refusal"
- file: .engineering/planning/story/view-filter-always-and-key.md
  line: 42
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome admits a key path compared with another field, listed and defined, but the scenarios only compare against a parameter or a literal"
```

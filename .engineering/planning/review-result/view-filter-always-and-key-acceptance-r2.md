---
format: aep.planning-md/3
id: review-result:view-filter-always-and-key-acceptance-r2
kind: review-result
status: active
title: Plan critic (acceptance), round 2, on the always and key filter story
relations:
- reviews: story:view-filter-always-and-key
revision: 1
---
needs-revision

Round 1: the key path compared with another field, listed and `defined` is fixed (`Listed`, `Compared`); the refusal boundary is fixed for paths (`Counted`) and still open for leaves.

- story:view-filter-always-and-key — no scenario submits another leaf to show it is still refused whole.
- story:view-filter-always-and-key — nested paths, `Optional` and newtype traversal and non-identity struct fields are claimed, and the fixture holds only the flat key struct.
- story:view-filter-always-and-key — "every read of every other view" does not name its reads.

```findings
- file: .engineering/planning/story/view-filter-always-and-key.md
  line: 44
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome says any other leaf is still refused whole, but no scenario submits any other leaf"
- file: .engineering/planning/story/view-filter-always-and-key.md
  line: 38
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome admits nested paths through Optional and newtypes and the members of every struct field, but the fixture holds only the key struct of plain strings"
- file: .engineering/planning/story/view-filter-always-and-key.md
  line: 106
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the scenario 'every read of every other view' does not name which reads or views"
```

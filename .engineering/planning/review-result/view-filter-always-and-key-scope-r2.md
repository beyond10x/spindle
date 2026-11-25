---
format: aep.planning-md/3
id: review-result:view-filter-always-and-key-scope-r2
kind: review-result
status: active
title: Plan critic (scope), round 2, on the always and key filter story
relations:
- reviews: story:view-filter-always-and-key
revision: 1
---
needs-revision

Round 1 is fixed: "Deliberately wider than the request" marks `never`/`always` everywhere and the members of every struct field, and ties `Named`, `Listed` and `Compared` to them. All five promises of the request are traced.

- story:view-filter-always-and-key — paths of any depth and traversal through `Optional` and newtypes are neither marked as extras nor proved by a fixture view.

```findings
- file: .engineering/planning/story/view-filter-always-and-key.md
  line: 38
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the outcome admits paths of any depth and traversal through Optional and newtypes, but the deliberate-extras section does not name them and the flat fixture key proves neither"
```

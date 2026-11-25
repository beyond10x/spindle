---
format: aep.planning-md/3
id: review-result:view-paging-scope-r3
kind: review-result
status: active
title: Plan critic (scope), round 3, on the view paging story
relations:
- reviews: story:view-paging
revision: 1
---
needs-revision

Round-2 finding resolved. The Outcome promises the page and size are read under the names paging declares, and every view used page and size, so a hardcoded key would pass.

```findings
- {file: .engineering/planning/story/view-paging.md, line: 42, category: scope, severity: warning, verdict: needs-revision, origin: introduced, message: "no view declares paging names other than page and size"}
```

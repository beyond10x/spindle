---
format: aep.planning-md/3
id: review-result:view-paging-design-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on the view paging story
relations:
- reviews: story:view-paging
revision: 1
---
needs-revision

story:view-read-families counts the rows Host::list_with returns, which this story changes, and no edge or Scope sentence ordered the two. No cycle; the story is cut end to end.

```findings
- {file: .engineering/planning/story/view-read-families.md, line: 43, category: design, severity: blocker, verdict: needs-revision, origin: introduced, message: "view-read-families counts list_with rows that view-paging changes, with no edge"}
- {file: .engineering/planning/story/view-paging.md, line: 104, category: design, severity: blocker, verdict: needs-revision, origin: introduced, message: "the Scope does not say which count a paged read has"}
```

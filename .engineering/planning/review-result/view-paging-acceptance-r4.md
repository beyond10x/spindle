---
format: aep.planning-md/3
id: review-result:view-paging-acceptance-r4
kind: review-result
status: active
title: Plan critic (acceptance), round 4, on the view paging story
relations:
- reviews: story:view-paging
revision: 1
---
needs-revision

Round-3 finding fixed; the size-3 overflow and the p/n reads are true against ESS 0.99.1. The FilteredNotePages result held only with first_page 0, and the view had no paging declaration in the story.

```findings
- {file: .engineering/planning/story/view-paging.md, line: 77, category: acceptance, severity: warning, verdict: needs-revision, origin: introduced, message: "FilteredNotePages has no paging declaration or first_page"}
```

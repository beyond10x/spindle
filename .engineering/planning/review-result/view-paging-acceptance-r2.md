---
format: aep.planning-md/3
id: review-result:view-paging-acceptance-r2
kind: review-result
status: active
title: Plan critic (acceptance), round 2, on the view paging story
relations:
- reviews: story:view-paging
revision: 1
---
needs-revision

Round-1 fixes landed. Two acceptance lines joined two outcomes each, and FilteredNotePages named no filter outcome, so a filter admitting every row would pass.

```findings
- {file: .engineering/planning/story/view-paging.md, line: 88, category: acceptance, severity: warning, verdict: needs-revision, origin: introduced, message: "total line joins two outcomes"}
- {file: .engineering/planning/story/view-paging.md, line: 76, category: acceptance, severity: blocker, verdict: needs-revision, origin: introduced, message: "FilteredNotePages filter and its total are not named"}
- {file: .engineering/planning/story/view-paging.md, line: 94, category: acceptance, severity: warning, verdict: needs-revision, origin: introduced, message: "conformance target line joins two outcomes and names no page or size"}
```

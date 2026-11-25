---
format: aep.planning-md/3
id: review-result:view-paging-acceptance-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on the view paging story
relations:
- reviews: story:view-paging
revision: 1
---
needs-revision

Two acceptance lines named no view that could run them (no paged view declared first_page 1 or a filter), the interpreter comparison named neither how the interpreter's page is obtained nor the sizes, and nothing checked that total is absent when undeclared or the conformance target.

```findings
- {file: .engineering/planning/story/view-paging.md, line: 77, category: acceptance, severity: blocker, verdict: needs-revision, origin: introduced, message: "no view declares first_page 1 and no sizes are named"}
- {file: .engineering/planning/story/view-paging.md, line: 77, category: acceptance, severity: warning, verdict: needs-revision, origin: introduced, message: "how the interpreter page is obtained and which sizes are covered is not said"}
- {file: .engineering/planning/story/view-paging.md, line: 82, category: acceptance, severity: blocker, verdict: needs-revision, origin: introduced, message: "the filtered-paging test names no view and the fixture has no filtered paged view"}
- {file: .engineering/planning/story/view-paging.md, line: 42, category: acceptance, severity: warning, verdict: needs-revision, origin: introduced, message: "total-only-when-declared and the conformance target are promised and not checked"}
```

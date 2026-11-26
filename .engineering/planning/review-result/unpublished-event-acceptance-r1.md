---
format: aep.planning-md/3
id: review-result:unpublished-event-acceptance-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on the unpublished event story
relations:
- reviews: story:command-with-unpublished-event-served
revision: 1
---
needs-revision

The no-envelope line named no accessor and read the same before and after; the served line named no status or body; two lines bundled several outcomes.

```findings
- {file: .engineering/planning/story/command-with-unpublished-event-served.md, line: 77, category: acceptance, severity: blocker, verdict: needs-revision, origin: introduced, message: "no accessor for staged no envelope, and no transition"}
- {file: .engineering/planning/story/command-with-unpublished-event-served.md, line: 72, category: acceptance, severity: warning, verdict: needs-revision, origin: introduced, message: "no status or body named"}
- {file: .engineering/planning/story/command-with-unpublished-event-served.md, line: 74, category: acceptance, severity: warning, verdict: needs-revision, origin: introduced, message: "two outcomes in one statement"}
- {file: .engineering/planning/story/command-with-unpublished-event-served.md, line: 79, category: acceptance, severity: warning, verdict: needs-revision, origin: introduced, message: "several outcomes in one statement"}
```

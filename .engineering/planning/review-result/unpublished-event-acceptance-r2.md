---
format: aep.planning-md/3
id: review-result:unpublished-event-acceptance-r2
kind: review-result
status: active
title: Plan critic (acceptance), round 2, on the unpublished event story
relations:
- reviews: story:command-with-unpublished-event-served
revision: 1
---
needs-revision

Round-1 fixes hold against the code. The body used the short event name where the envelope writes the qualified one, the flush test did not say how t1 exists, and the interpreter line named no accessor.

```findings
- {file: .engineering/planning/story/command-with-unpublished-event-served.md, line: 79, category: acceptance, severity: blocker, verdict: needs-revision, origin: introduced, message: "short event name where the envelope writes the qualified name"}
- {file: .engineering/planning/story/command-with-unpublished-event-served.md, line: 84, category: acceptance, severity: warning, verdict: needs-revision, origin: introduced, message: "how t1 exists is not said"}
- {file: .engineering/planning/story/command-with-unpublished-event-served.md, line: 81, category: acceptance, severity: warning, verdict: needs-revision, origin: introduced, message: "no interpreter accessor named"}
```

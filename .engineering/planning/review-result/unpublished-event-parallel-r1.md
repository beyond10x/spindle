---
format: aep.planning-md/3
id: review-result:unpublished-event-parallel-r1
kind: review-result
status: active
title: Plan critic (parallel-safety), round 1, on the unpublished event story
relations:
- reviews: story:command-with-unpublished-event-served
revision: 1
---
needs-revision

Shared files named two stories that do not edit host.rs, and missed story:committed-events-survive-restart, whose restart replay is a second caller of Sender::publish.

```findings
- {file: .engineering/planning/story/command-with-unpublished-event-served.md, line: 109, category: parallel-safety, severity: warning, verdict: needs-revision, origin: introduced, message: "request-span-tree and spindle-spec-cites-source-by-symbol do not edit host.rs"}
- {file: .engineering/planning/story/command-with-unpublished-event-served.md, line: 89, category: parallel-safety, severity: warning, verdict: needs-revision, origin: introduced, message: "committed-events-survive-restart replays through Sender::publish, unrecorded"}
```

---
format: aep.planning-md/3
id: review-result:node-export-fixed-actor-parallel-r1
kind: review-result
status: active
title: Plan critic (parallel-safety), round 1, on the Node export fixed actor story
relations:
- reviews: story:node-export-fixed-actor
revision: 1
---
needs-revision

The story shares files with open stories and its Scope section did not say so.

```findings
- file: .engineering/planning/story/node-export-fixed-actor.md
  line: 75
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "story:service-worker-host-matches-run shares crates/spindle-wasm/src/worker.rs and reads the Opening::callers and Surface::fixing_callers this story creates, with no depends_on edge from it to this story"
- file: .engineering/planning/story/node-export-fixed-actor.md
  line: 70
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "crates/spindle-ingress/src/surface.rs is also in story:front-ingress and story:cross-component-view-refused, and crates/spindle-cli/src/serve.rs is also in story:front-ingress, story:otlp-trace-export and story:request-span-tree; the Scope section does not say so"
- file: .engineering/planning/story/node-export-fixed-actor.md
  line: 82
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "AGENTS.md, README.md and website/docs/concepts/environment-and-admission.md are shared with story:node-export-release-artifact, story:spindle-image-published and story:reset-reapplies-seed without naming them"
- file: .engineering/planning/story/node-export-release-artifact.md
  line: 32
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its acceptance runs crates/spindle-node/tests/node/*.test.mjs, which includes fixed-actor.test.mjs, with no depends_on edge to story:node-export-fixed-actor"
```

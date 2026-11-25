---
format: aep.planning-md/3
id: review-result:node-export-fixed-actor-design-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on the Node export fixed actor story
relations:
- reviews: story:node-export-fixed-actor
revision: 1
---
needs-revision

The story itself holds: no cycle in the 22 declared edges around the node-export set, the edge to `story:node-export-clock-and-seeds` carries its reason, the surfaces it shares with `story:node-export-serves-a-specification` are ordered by its `depends_on` edge, and it is cut vertically (Node lane, native recording, `spindle-ingress`, `spindle-embed`).

One finding on a sibling, pre-existing: the acceptance of `story:service-worker-host-matches-run` reads `Opening::callers`, `fixed` and `Surface::fixing_callers`, which this story creates, and no `depends_on` edge records that order.

```findings
- file: .engineering/planning/story/service-worker-host-matches-run.md
  line: 46
  category: design
  severity: warning
  verdict: needs-revision
  origin: pre-existing
  message: "its acceptance reads Opening::callers, fixed and Surface::fixing_callers that story:node-export-fixed-actor creates, and no depends_on edge runs from it to story:node-export-fixed-actor"
```

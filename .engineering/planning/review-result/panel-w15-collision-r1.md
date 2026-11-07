---
format: aep.planning-md/3
id: review-result:panel-w15-collision-r1
kind: review-result
status: active
title: Route template collision draft, plan-critic panel, round 1
relations:
- reviews: story:surface-cross-component-template-collision
revision: 1
---
approve

Plan-critic panel, round 1, on story:surface-cross-component-template-collision, drafted from finding F3 of review-result:w15-u5-adversary-1. All four lanes approve with no findings: acceptance, design, parallel-safety and scope. Notes the lanes raised, addressed before activation: the body cited `surface.rs` line 686 for the collision check, which is in `Surface::routed` at lines 733 to 741 at a8dd7e12; the story was linked to its source review. Notes left open: the acceptance test matches `SurfaceError::Collision { .. }` without checking that both components are named; note J1 of the same review (a crossed template pair served by two components) is claimed by no story.

```findings
[]
```

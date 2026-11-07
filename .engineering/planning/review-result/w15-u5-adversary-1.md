---
format: aep.planning-md/3
id: review-result:w15-u5-adversary-1
kind: review-result
status: active
title: Adversary pass 1 on route-literal-segment-precedence (wave 15)
relations:
- reviews: story:route-literal-segment-precedence
revision: 1
---
needs-change

Adversary pass 1 on story:route-literal-segment-precedence (wave 15 U5), attacking a41dd681. Four
failing cases in `crates/spindle-ingress/tests/spindle-ingress/adversary_route_precedence.rs`
(commit 8af88489); spindle-ingress tests 23 -> 27, 4 red. All four gaps were already in the parent.

- F1, in scope, fixed in the unit: a literal segment was compared with the raw request segment, so
  `/things/%7Emodules` and `/things/%6Dodules` reached the by-id route; ESS 0.90.0's precedence rule
  decodes percent-encoded unreserved characters first.
- F2, fixed in the unit: a 405 for a path two routes of one method match named the method twice in
  `Allow`.
- F3, out of scope, filed as story:surface-cross-component-template-collision: two components whose
  templates differ only in variable names are not refused as a collision; its test is ignored with
  that story's id.
- J1, note: across components a nested pair is now ranked by specificity, a crossed pair still goes
  to the earlier component. ESS defines no listener serving several components.

Could not break: `Surface::routes` and `Surface::unserved` order, the index mapping, trailing
slashes and empty segments, HEAD and OPTIONS, cost per request.

```findings
- file: crates/spindle-ingress/src/surface.rs
  line: 373
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: pre-existing
  message: "a literal segment was compared with the raw request segment, so percent-encoded unreserved characters reached the by-id route"
- file: crates/spindle-ingress/src/surface.rs
  line: 824
  category: boundary
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "a 405 named a method twice in Allow when two routes of it matched"
- file: crates/spindle-ingress/src/surface.rs
  line: 686
  category: boundary
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "two components whose templates differ only in variable names are not refused as a collision; filed as story:surface-cross-component-template-collision"
```

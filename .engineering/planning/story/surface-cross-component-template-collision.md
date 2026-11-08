---
format: aep.planning-md/3
id: story:surface-cross-component-template-collision
kind: story
status: implemented
title: Two components cannot claim one route template on one listener
relations:
- serves: vision:spindle
- informed_by: story:route-literal-segment-precedence
- informed_by: review-result:w15-u5-adversary-1
scope:
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
- confidence: cited
  path: crates/spindle-ingress/tests/spindle-ingress/adversary_route_precedence.rs
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-07T21:50:49Z", actor: "human:timo", revision: 5}
- {from: "proposed", to: "active", at: "2026-11-07T21:50:49Z", actor: "human:timo", revision: 6}
- {from: "active", to: "implemented", at: "2026-11-08T17:25:49Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"test_result":1,"verification":1}}}
---
# Two components cannot claim one route template on one listener

## Outcome

When several components are served on one listener, `Surface::new` refuses two routes of one
method whose templates name the same route under ESS's rule (ESS-HTTP-010: templates that differ
only in their variable names), answering `SurfaceError::Collision` naming both components. Today the
collision check compares raw template text (`crates/spindle-ingress/src/surface.rs`, `Surface::routed`, lines 733 to 741), so
`GET /things/{id}` in one component and `GET /things/{key}` in another are both accepted, the first
answers every request and the second can never be reached. ESS checks overlaps only within one
component.

## Acceptance

- `adversary_two_components_claiming_one_template_collide`
  (`crates/spindle-ingress/tests/spindle-ingress/adversary_route_precedence.rs`), ignored today with
  this story's id, runs and passes: `Surface::new` returns `SurfaceError::Collision`.

## Scope

- `crates/spindle-ingress/src/surface.rs`: cited. The collision check in `Surface::routed` (lines 733 to 741).
- `crates/spindle-ingress/tests/spindle-ingress/adversary_route_precedence.rs`: cited.

## Origin

Adversary pass 1 on story:route-literal-segment-precedence (wave 15), finding F3.

---
format: aep.planning-md/3
id: story:surface-cross-component-template-collision
kind: story
status: draft
title: Two components cannot claim one route template on one listener
relations:
- serves: vision:spindle
- informed_by: story:route-literal-segment-precedence
scope:
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
- confidence: cited
  path: crates/spindle-ingress/tests/spindle-ingress/adversary_route_precedence.rs
revision: 3
---
# Two components cannot claim one route template on one listener

## Outcome

When several components are served on one listener, `Surface::new` refuses two routes of one
method whose templates name the same route under ESS's rule (ESS-HTTP-010: templates that differ
only in their variable names), answering `SurfaceError::Collision` naming both components. Today the
collision check compares raw template text (`crates/spindle-ingress/src/surface.rs`, line 686), so
`GET /things/{id}` in one component and `GET /things/{key}` in another are both accepted, the first
answers every request and the second can never be reached. ESS checks overlaps only within one
component.

## Acceptance

- `adversary_two_components_claiming_one_template_collide`
  (`crates/spindle-ingress/tests/spindle-ingress/adversary_route_precedence.rs`), ignored today with
  this story's id, runs and passes: `Surface::new` returns `SurfaceError::Collision`.

## Scope

- `crates/spindle-ingress/src/surface.rs`: cited. The collision check (line 686).
- `crates/spindle-ingress/tests/spindle-ingress/adversary_route_precedence.rs`: cited.

## Origin

Adversary pass 1 on story:route-literal-segment-precedence (wave 15), finding F3.

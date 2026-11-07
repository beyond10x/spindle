---
format: aep.planning-md/3
id: story:ingress-error-object-bodies
kind: story
status: active
title: HTTP ingress answers bound error outcomes with an error-object body
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/error-object-bodies
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/error_object_bodies.rs
- confidence: inferred
  path: crates/spindle-ingress/src/answer.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-07T12:45:25Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-07T12:45:26Z", actor: "human:timo", revision: 5}
---
# HTTP ingress answers bound error outcomes with an error-object body

## Outcome

A refused outcome whose `ess-http` binding maps it to a status and an error object (for example 400
with field errors) is answered with that status and a body built from the outcome's error fields.
Today `crates/spindle-ingress/src/surface.rs` answers such a route 501.

## Acceptance

- `a_bound_error_outcome_answers_its_status_and_error_object` — a neutral fixture with one refused
  outcome bound to 400 and an error object answers 400 with the object, and writes nothing.

## Origin

Pre-existing. Reported by a consumer serving a test backend: 9 bound routes in its specification.

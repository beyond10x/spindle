---
format: aep.planning-md/3
id: story:console-api
kind: story
status: draft
title: A generic console API over the ingress
relations:
- serves: vision:spindle
- decomposes: epic:m0-run-in-process
- depends_on: story:run-over-http-ingress
scope:
- confidence: inferred
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-ingress
revision: 2
---
# A generic console API over the ingress

## Outcome

Beside the command and view routes `story:run-over-http-ingress` serves
(`POST /api/commands/{command}`, `GET /api/views/{view}`), a loopback-only run also serves what a
generic model console reads: `GET /api/model` (the compiled IR and each outcome's subject),
`GET /api/events` (the events published so far, in order) and `POST /api/reset` (empty every store
of the run). Requested by a downstream session whose console already reads that shape
(2026-11-05).

## Acceptance

- `the_model_route_answers_the_compiled_ir` — `GET /api/model` answers the IR `spindle-spec`
  compiled, byte-equal to `ess specify compile --format json` for the same specification, plus
  outcome subjects.
- `the_events_route_lists_published_events_in_order`.
- `reset_empties_every_store_of_the_run` — after a reset every view answers no rows.
- `reset_is_refused_outside_loopback` — the route exists only with `ingress.network` bound to
  `http-loopback`.

## Scope

- `crates/spindle-ingress` — inferred: lands with `story:run-over-http-ingress`.
- `crates/spindle-host/src/host.rs` — inferred: an event log read and a reset.

## Depends on

- `story:run-over-http-ingress`.

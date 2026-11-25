---
format: aep.planning-md/3
id: story:wasi-timers-driven
kind: story
status: draft
title: Periodic bindings fire in a WASI module
relations:
- serves: vision:spindle
- decomposes: epic:wasi-server-module
- depends_on: story:wasi-state-survives-instances
revision: 1
---
# Periodic bindings fire in a WASI module

## Outcome

A WASI module fires the specification's periodic bindings as a native run does: something calls
`Host::advance` (`crates/spindle-host/src/host.rs:1508`) as WASI's clock passes each binding's
time — a scheduled request from the WASI host, or a poll the component runs between requests,
whichever `story:wasi-state-survives-instances` leaves possible.

## Acceptance

- `a_periodic_binding_fires_under_wasmtime` — a fixture with a periodic binding: no tick before
  its interval, one after it, the event visible at `GET /api/events`.

## Depends on

- `story:wasi-state-survives-instances`.

## Scope

- The component crate (inferred).

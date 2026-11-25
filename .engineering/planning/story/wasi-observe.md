---
format: aep.planning-md/3
id: story:wasi-observe
kind: story
status: draft
title: A WASI module answers the observe routes
relations:
- serves: vision:spindle
- decomposes: epic:wasi-server-module
- depends_on: story:wasi-state-survives-instances
revision: 1
---
# A WASI module answers the observe routes

## Outcome

A WASI module observes as `run --observe` does: the surface records each answered request
(`Surface::observing`), and `/__spindle/requests` and `/__spindle/metrics` answer as natively,
the records kept where `story:wasi-state-survives-instances` keeps state.

## Acceptance

- `the_observe_routes_answer_under_wasmtime` — after the review fixture's exchanges,
  `GET /__spindle/requests` lists them and `GET /__spindle/metrics` counts them, in the shapes the
  observe API fixtures (`ess/fixtures/observe/`) validate.

## Depends on

- `story:wasi-state-survives-instances`.

## Scope

- The component crate (inferred).

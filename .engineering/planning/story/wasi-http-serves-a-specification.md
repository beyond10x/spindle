---
format: aep.planning-md/3
id: story:wasi-http-serves-a-specification
kind: story
status: draft
title: A WASI module serves a specification over wasi:http
relations:
- serves: vision:spindle
- decomposes: epic:wasi-server-module
- depends_on: story:host-opening-is-transport-free
revision: 1
---
# A WASI module serves a specification over wasi:http

## Outcome

A `wasm32-wasip2` component exports `wasi:http/incoming-handler`: wasmtime (`wasmtime serve`)
hands it each request, the component routes it through `spindle_ingress::route_with` on a host
opened from texts (`story:host-opening-is-transport-free`), console routes included, and answers
with the status, headers and body the loopback listener writes. The configuration (sources,
environment, decisions, binding) comes from files the WASI host preopens.

## Acceptance

- `a_wasi_component_answers_the_recorded_exchanges` — under `wasmtime serve` on loopback, every
  exchange of `crates/spindle-wasm/tests/fixtures/review/exchanges.json` gets the native answer.
- An environment binding `ingress.network` to anything but the WASI implementation is refused at
  start, naming it.

## Depends on

- `story:host-opening-is-transport-free`.

## Scope

- A new crate for the component (inferred).

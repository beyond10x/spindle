---
format: aep.planning-md/3
id: story:wasmtime-deploy-target
kind: story
status: draft
title: wasmtime is a deployment target
relations:
- serves: vision:spindle
- decomposes: epic:wasi-server-module
- depends_on: story:wasi-state-survives-instances
revision: 1
---
# wasmtime is a deployment target

## Outcome

A placement can put a component on a wasmtime target beside another server process: `deploy`
launches `wasmtime serve` with the component, its preopened configuration and its port, through
the `Launcher` contract (`crates/spindle-target`), and the control process collects its
telemetry as it does for a process target.

## Acceptance

- `a_component_placed_on_wasmtime_answers` — `deploy` with a placement naming the target starts
  the component; a request to its port is answered; stopping the deployment stops it.

## Depends on

- `story:wasi-state-survives-instances`.

## Scope

- `crates/spindle-target`, `crates/spindle-control`, `examples/placements/` (inferred).

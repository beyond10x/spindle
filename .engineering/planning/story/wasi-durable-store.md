---
format: aep.planning-md/3
id: story:wasi-durable-store
kind: story
status: draft
title: A durable store on WASI
relations:
- serves: vision:spindle
- decomposes: epic:wasi-server-module
- depends_on: story:wasi-host-builds
revision: 1
---
# A durable store on WASI

## Outcome

A `Store` that keeps state across restarts of the WASI module, bound by `store.durable` to its own
implementation name: on `wasi:keyvalue`, over outbound HTTP to a store beside it, or SQLite on a
preopened directory compiled for WASI. It passes `spindle_kernel::contract`, the cases every store
passes.

## Acceptance

- The store passes `spindle_kernel::contract` under wasmtime.
- `state_survives_a_restart` — a row written, the module stopped and started, the row read back.

## Depends on

- `story:wasi-host-builds`.

## Scope

- A new store crate (inferred).

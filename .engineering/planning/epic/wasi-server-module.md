---
format: aep.planning-md/3
id: epic:wasi-server-module
kind: epic
status: draft
title: 'Spindle as a WASI server module under wasmtime beside another server process: listener, store and clock on WASI'
relations:
- serves: vision:spindle
revision: 1
---
# Spindle as a WASI server module

## Outcome

Spindle's host runs as a WASI module under wasmtime, beside another server process on the same
machine: a specification served over `wasi:http`, its state in a store a WASI host provides, its
timers driven by a WASI clock, and deployable as a target like the process and microVM targets.
Nothing of it is built yet; these drafts record the work and its order.

## What stands in the way today (2026-11-25, main 7bcc325ed)

- The SQLite store runs on `eventlog` with `tokio` and blocks on it
  (`crates/spindle-kernel/src/sqlite.rs:219`, `:288`).
- The loopback listener binds `std::net`, starts a thread and runs `tokio::net`
  (`crates/spindle-ingress/src/listener.rs:313`, `:332`, `:336`).
- `b10x-spindle` uses `signal-hook`; the target, QEMU and Firecracker crates launch processes or
  VMs.
- The host's opening from texts (compile, admit, decisions, binding, surface, stores) sits behind
  the wasm-bindgen gate of `crates/spindle-wasm` until
  `story:node-export-serves-a-specification` moves it into `crates/spindle-embed`.

## What is in place

- The `Store` trait is synchronous (`crates/spindle-kernel/src/store.rs:207`) with a contract every
  store passes; `Host::supplied` takes any stores, clock and decider
  (`crates/spindle-host/src/host.rs:711`); the router is transport-free
  (`story:http-router-is-transport-free`).
- The clock and `uuid`'s `js` feature switch only on `target_os = "unknown"`
  (`crates/spindle-timer/src/clock.rs:10-22`, `crates/spindle-kernel/Cargo.toml:33-36`), so on
  `wasm32-wasip2` std's clock and randomness apply (inferred, not built).

## Stories, in dependency order

1. `story:wasi-host-builds` — the host builds for `wasm32-wasip2`.
2. `story:host-opening-is-transport-free` (1).
3. `story:wasi-http-serves-a-specification` (2).
4. `story:wasi-state-survives-instances` (3) — the design decision of this epic.
5. `story:wasi-durable-store` (1).
6. `story:wasi-timers-driven` (4).
7. `story:wasi-observe` (4).
8. `story:wasi-seed-and-reset-world` (4).
9. `story:wasi-externals-over-http` (3).
10. `story:wasi-conformance-lane` (3).
11. `story:wasmtime-deploy-target` (4).

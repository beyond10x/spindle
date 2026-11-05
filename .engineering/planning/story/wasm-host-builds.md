---
format: aep.planning-md/3
id: story:wasm-host-builds
kind: story
status: draft
title: The host builds for wasm32
relations:
- serves: vision:spindle
- decomposes: epic:browser-host
scope:
- confidence: inferred
  path: Taskfile.yml
- confidence: inferred
  path: crates/spindle-host/Cargo.toml
- confidence: inferred
  path: crates/spindle-kernel/Cargo.toml
- confidence: inferred
  path: crates/spindle-kernel/src/sqlite.rs
revision: 2
---
# The host builds for wasm32

## Outcome

`spindle-spec`, `spindle-kernel` and `spindle-host` build for `wasm32-unknown-unknown` with a
feature set that leaves out the native-only parts (SQLite through `eventlog`, `tokio`, process and
VM launchers), and the memory store runs a command there. Randomness and the wall clock come from
the browser (the `uuid` `js` feature or a `Mint` and `Clock` passed in).

## Acceptance

- `the_wasm_build_of_the_host_succeeds` — `cargo build --target wasm32-unknown-unknown` of the
  three crates with the wasm feature set, run in `task check`.
- `a_command_runs_in_wasm_on_the_memory_store` — a `wasm-bindgen-test` (or node-run wasm test)
  executes a command of a neutral fixture and reads its view.
- `the_native_build_is_unchanged` — the default features still build and pass the workspace tests.

## Scope

- `crates/spindle-kernel/Cargo.toml`, `crates/spindle-host/Cargo.toml` — inferred: features.
- `crates/spindle-kernel/src/sqlite.rs` — inferred: behind a native feature.
- `Taskfile.yml` — inferred: a wasm build step.

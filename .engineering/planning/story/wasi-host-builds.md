---
format: aep.planning-md/3
id: story:wasi-host-builds
kind: story
status: draft
title: The host builds for wasm32-wasip2
relations:
- serves: vision:spindle
- decomposes: epic:wasi-server-module
revision: 1
---
# The host builds for wasm32-wasip2

## Outcome

`spindle-spec`, `spindle-kernel` (memory store), `spindle-host`, `spindle-ingress` (router only),
`spindle-timer`, `spindle-channel` and `spindle-capability` build for `wasm32-wasip2` without
SQLite, the listener, `tokio` or `signal-hook`, std's clock and randomness coming from WASI.

## Acceptance

- `task check:the_wasi_build_of_the_host_succeeds` — `cargo build` and `cargo clippy` for
  `wasm32-wasip2 --no-default-features` of those crates pass in `task check` and CI.
- A command runs on the memory store under wasmtime (`cargo test` with a wasmtime runner), as
  `check:a_command_runs_in_wasm_on_the_memory_store` does on Node.

## Scope

- The crates' `Cargo.toml` target tables, `Taskfile.yml`, `.github/workflows/ci.yml` (inferred).

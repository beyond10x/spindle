---
format: aep.planning-md/3
id: story:host-opening-is-transport-free
kind: story
status: draft
title: Opening a host from texts builds for every wasm target
relations:
- serves: vision:spindle
- decomposes: epic:wasi-server-module
- depends_on: story:wasi-host-builds
- informed_by: story:node-export-serves-a-specification
revision: 1
---
# Opening a host from texts builds for every wasm target

## Outcome

The opening a wasm host performs — compile the sources, admit them on the environment, read the
decisions file and the binding, lay out the surface with the console routes, open the host on
supplied stores — builds for `wasm32-wasip2` as it builds for the browser and Node, so a WASI
module opens a host with the same code and the same refusals.

`story:node-export-serves-a-specification` moves this opening out of the wasm-bindgen gate of
`crates/spindle-wasm` into `crates/spindle-embed`, built native and `wasm32-unknown-unknown`.
What is left here is `wasm32-wasip2`: the crate's dependencies and its clock and randomness on WASI.

## Acceptance

- `spindle-embed` builds and lints for `wasm32-wasip2` in the lane of `story:wasi-host-builds`.
- Its native tests answer the recorded exchanges
  (`crates/spindle-wasm/tests/fixtures/review/exchanges.json`) as the loopback listener does.

## Depends on

- `story:wasi-host-builds`; informed by `story:node-export-serves-a-specification`, which does
  the move.

## Scope

- `crates/spindle-embed/Cargo.toml` (inferred).

---
format: aep.planning-md/3
id: epic:browser-host
kind: epic
status: draft
title: Spindle runs in the browser
relations:
- serves: vision:spindle
revision: 1
---
# Spindle runs in the browser

## Outcome

A static site can host a specification's console with a working backend that never leaves the
browser: Spindle's host compiled to WebAssembly answers the specification's `ess-http/1` routes and
the console routes from a Service Worker, with state in browser storage and external outcomes
answered from a decisions file. Requested 2026-11-05 through a downstream session.

## Stories

- `story:wasm-host-builds` — `spindle-spec`, `spindle-kernel` and `spindle-host` build for
  `wasm32-unknown-unknown`.
- `story:browser-store` — a `Store` backed by IndexedDB behind the existing trait.
- `story:http-router-is-transport-free` — the wave 4 ingress router split from its listener, so
  the same routing answers a Service Worker request.
- `story:service-worker-serves-a-specification` — the Service Worker answering the bound routes
  and the console routes from the wasm host, decisions from a file.

## Rules

- Browser code is Vue 3 and TypeScript (the Service Worker shell and its registration); the
  routing, kernel and host are the Rust crates compiled to wasm, not a second implementation.
- The console routes are console-only, as in `story:console-api`.

## Known

- 2026-11-05, on 22b2293: `cargo check --target wasm32-unknown-unknown -p spindle-spec` passes;
  `-p spindle-kernel` fails first on `uuid` needing a randomness source. `eventlog`/SQLite and
  `tokio` were not checked past that.

---
format: aep.planning-md/3
id: story:browser-store
kind: story
status: active
title: A browser store
relations:
- serves: vision:spindle
- decomposes: epic:browser-host
- depends_on: story:wasm-host-builds
scope:
- confidence: inferred
  path: crates/spindle-store-browser
- confidence: inferred
  path: examples/environments/browser.toml
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-05T09:39:08Z", actor: "human:timo", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-05T09:39:08Z", actor: "human:timo", revision: 4}
---
# A browser store

## Outcome

A `Store` implementation backed by IndexedDB, behind the existing `Store` trait, so a run in the
browser keeps its state across page reloads. Chosen by the `store.durable` binding in a browser
environment document.

## Acceptance

- `state_survives_a_reload` — a command's effect is read back after the store is reopened.
- `the_browser_store_passes_the_store_contract` — the same store tests the memory and SQLite
  stores pass.

## Scope

- `crates/spindle-store-browser` — inferred, new.
- `examples/environments/browser.toml` — inferred, new.

## Depends on

- `story:wasm-host-builds`.

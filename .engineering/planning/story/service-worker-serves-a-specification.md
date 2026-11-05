---
format: aep.planning-md/3
id: story:service-worker-serves-a-specification
kind: story
status: active
title: A Service Worker serves a specification
relations:
- serves: vision:spindle
- decomposes: epic:browser-host
- depends_on: story:browser-store
- depends_on: story:http-router-is-transport-free
- depends_on: story:console-api
scope:
- confidence: inferred
  path: crates/spindle-wasm
- confidence: inferred
  path: web
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-05T09:39:08Z", actor: "human:timo", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-05T09:39:08Z", actor: "human:timo", revision: 4}
---
# A Service Worker serves a specification

## Outcome

A static site registers a Service Worker that loads the wasm host with a compiled specification,
a browser environment and a decisions file, and answers the specification's routes and the
console routes, so the console's fetch calls never leave the browser. External outcomes are
answered from the decisions file (`story:external-decisions-file`).

## Acceptance

- `a_fetch_to_a_bound_route_is_answered_in_the_browser` — a headless browser test: no network
  request leaves the page, and the answer matches the loopback listener's for the same request.
- `an_external_outcome_is_answered_from_the_decisions_file`.
- `the_static_bundle_builds` — the site bundle (Vite, pnpm) builds in `task check`.

## Scope

- `web/` — inferred, new: the Vue/TypeScript shell and Service Worker registration.
- `crates/spindle-wasm` — inferred, new: the wasm-bindgen entry.

## Depends on

- `story:browser-store`, `story:http-router-is-transport-free`, `story:console-api`.

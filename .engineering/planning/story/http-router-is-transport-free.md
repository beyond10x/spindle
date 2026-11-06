---
format: aep.planning-md/3
id: story:http-router-is-transport-free
kind: story
status: implemented
title: The HTTP router is transport-free
relations:
- serves: vision:spindle
- decomposes: epic:browser-host
- depends_on: story:run-over-http-ingress
- depends_on: story:wasm-host-builds
scope:
- confidence: inferred
  path: crates/spindle-ingress
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-05T09:39:08Z", actor: "human:timo", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-05T09:39:08Z", actor: "human:timo", revision: 4}
- {from: "active", to: "implemented", at: "2026-11-06T19:57:14Z", actor: "human:timo", revision: 5, decided_on: {"recorded":{"test_result":1}}}
---
# The HTTP router is transport-free

## Outcome

The routing of `story:run-over-http-ingress` (an `ess-http/1` binding's routes, or the fallback
routes, to commands and views on a `Host`) is a function from a request to an answer with no
listener, socket or async runtime in it, so a Service Worker and the loopback listener call the
same code.

## Acceptance

- `the_router_answers_without_a_listener` — a request value routed against a host answers the
  same status and body the listener test observes.
- `the_router_builds_for_wasm32` — with `story:wasm-host-builds`' feature set.

## Scope

- `crates/spindle-ingress` — inferred: lands in wave 4.

## Depends on

- `story:run-over-http-ingress`, `story:wasm-host-builds`.

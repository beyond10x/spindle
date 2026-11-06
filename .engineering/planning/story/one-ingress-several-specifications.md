---
format: aep.planning-md/3
id: story:one-ingress-several-specifications
kind: story
status: draft
title: One ingress serves several specifications
relations:
- serves: vision:spindle
revision: 1
---
# One ingress serves several specifications

## Outcome

One listener holds the `ess-http` binding of every specification it is given and routes each
request to the component that owns it; events between the specifications flow in process. Today
one `run` serves one specification (`crates/spindle-cli/src/serve.rs`).

## Acceptance

- `one_listener_routes_to_two_specifications` — two neutral specifications, one route each, both
  answered by one listener; an event published by one is consumed by the other.
- Two bindings claiming one route are refused at start, naming both.

## Origin

Pre-existing. Requested by a consumer that serves a backend and a pub/sub specification behind one
UI.

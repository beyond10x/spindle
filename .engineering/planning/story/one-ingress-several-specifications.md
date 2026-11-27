---
format: aep.planning-md/3
id: story:one-ingress-several-specifications
kind: story
status: archived
title: One ingress serves several specifications
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "archived", at: "2026-11-27T09:56:39Z", actor: "human:operator", revision: 4}
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

## Architecture decision

Option B, picked by the operator on 2026-11-27: no change. A consumer that needs one listener for
two specifications lists both domains in one `system.yaml`; events between them already flow in
process. The story is archived.

Not taken: A (`run` takes several specification directories, with a new binding format for events
across them), C (no change; a proxy routes by path to two runs, events over the webhook of
`story:event-egress-and-dead-letters`), D (defer).

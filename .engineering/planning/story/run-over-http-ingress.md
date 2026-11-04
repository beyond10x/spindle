---
format: aep.planning-md/3
id: story:run-over-http-ingress
kind: story
status: active
title: Run over HTTP ingress
relations:
- serves: vision:spindle
- decomposes: epic:m0-run-in-process
- depends_on: story:views-with-filters-and-lists
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 3}
---
# Run over HTTP ingress

## Outcome

`b10x-spindle run` with `ingress.network = http-loopback` serves the components' accepted commands and views over HTTP on 127.0.0.1, request and answer shapes from the ESS projections.

## Acceptance

- `a_command_posted_over_http_executes_and_answers`
- `a_view_is_answered_over_http`
- `the_listener_binds_loopback_only`

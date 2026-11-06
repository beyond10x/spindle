---
format: aep.planning-md/3
id: story:ingress-delete-routes
kind: story
status: implemented
title: HTTP ingress serves commands bound to DELETE
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-cli/tests/ingress.rs
- confidence: inferred
  path: crates/spindle-ingress/src/listener.rs
- confidence: inferred
  path: crates/spindle-ingress/src/surface.rs
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T00:36:51Z", actor: "agent:b10x-chef-orchestrator", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-06T00:36:51Z", actor: "agent:b10x-chef-orchestrator", revision: 4}
- {from: "active", to: "implemented", at: "2026-11-06T08:35:38Z", actor: "human:timo", revision: 5, decided_on: {"recorded":{"test_result":2}}}
---
# HTTP ingress serves commands bound to DELETE

## Outcome

A command an HTTP binding puts on `method: delete` is served by `run` serving `ingress.network`:
a `DELETE` request to its path runs the command with the input its path (and, with
`story:ingress-form-and-query-requests`, its query string) carries, and answers the status the
binding lists, as ESS publishes it in the OpenAPI document. ESS 0.86.0 admits `delete` routes and
a `query` request on them (`ESS-HTTP-020`). No test in spindle serves one today, so whether such a
route resolves and runs end to end is unverified; a `DELETE` with no body must be read as the
empty input, not refused.

## Acceptance

- `a_command_bound_to_delete_is_served` — a binding puts `RemoveBook` on `DELETE /books/{id}`:
  the request answers the listed status and the entity's view no longer lists the row.
- `a_delete_without_a_body_is_the_path_input` — a `DELETE` with no body and no content type runs
  with the input the path carries.
- `a_get_to_a_delete_route_is_refused_405` — the same path asked with `GET` answers `405` with an
  `Allow: DELETE` header.

## Origin

Requested by a consumer that serves a specification over HTTP in memory as a test backend for a
browser UI suite, 2026-11-06.

## Scope

- `crates/spindle-ingress/src/surface.rs` — inferred: route method and resolution.
- `crates/spindle-ingress/src/listener.rs` — inferred: reading a request with no body.
- `crates/spindle-cli/tests/ingress.rs` — inferred: the end-to-end tests.

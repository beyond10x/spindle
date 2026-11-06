---
format: aep.planning-md/3
id: story:ingress-view-parameters
kind: story
status: active
title: Views with path or query parameters are served
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-host/src/view.rs
- confidence: inferred
  path: crates/spindle-ingress/src/answer.rs
- confidence: inferred
  path: crates/spindle-ingress/src/surface.rs
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T00:36:51Z", actor: "agent:b10x-chef-orchestrator", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-06T00:36:51Z", actor: "agent:b10x-chef-orchestrator", revision: 5}
---
# Views with path or query parameters are served

## Outcome

A view that declares parameters (such as a get-by-id view whose parameter is bound to a path
variable or the query string) is served: the parameters are read from the request as the HTTP
binding places them and the view answers the rows ESS's interpreter answers for those values.
Today the host refuses any view with parameters (`crates/spindle-host/src/view.rs`, `unsupported`:
"parameters"), so the ingress cannot serve it.

## Acceptance

- `a_view_parameter_in_the_path_selects_the_row` — `GET /books/{id}` over a view with an `id`
  parameter answers `{"rows": [<that book>]}`, and an unknown id answers `{"rows": []}`.
- `a_view_parameter_in_the_query_filters_the_rows` — `GET /books?state=Lent` answers only the
  lent books, the rows ESS's interpreter answers.
- `a_malformed_view_parameter_is_refused_400` — a parameter whose text is not its declared type
  is refused `400`, naming the parameter.
- `a_missing_required_view_parameter_is_refused_400` — a required query parameter left out is
  refused `400`.

## Origin

Requested by a consumer that serves a specification over HTTP in memory as a test backend for a
browser UI suite, 2026-11-06.

## Scope

- `crates/spindle-host/src/view.rs` — cited: the "parameters" refusal in `unsupported`.
- `crates/spindle-host/src/host.rs` — inferred: `view` and `list` taking parameter values.
- `crates/spindle-ingress/src/surface.rs` — inferred: view variables from the path and query.
- `crates/spindle-ingress/src/answer.rs` — inferred: answering a view with its parameters.

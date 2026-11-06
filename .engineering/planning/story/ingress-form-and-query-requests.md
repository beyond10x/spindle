---
format: aep.planning-md/3
id: story:ingress-form-and-query-requests
kind: story
status: implemented
title: Form- and query-encoded command requests are accepted
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-ingress/src/answer.rs
- confidence: inferred
  path: crates/spindle-ingress/src/listener.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T00:36:51Z", actor: "agent:b10x-chef-orchestrator", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-06T00:36:51Z", actor: "agent:b10x-chef-orchestrator", revision: 5}
- {from: "active", to: "implemented", at: "2026-11-06T13:06:14Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"test_result":2}}}
---
# Form- and query-encoded command requests are accepted

## Outcome

A command whose HTTP binding declares `request: {encoding: form}` or `{encoding: query}` is
served: the input is read from an `application/x-www-form-urlencoded` body or from the query
string, each flat field read by its declared type as path variables are today. Today
`crates/spindle-ingress/src/surface.rs` (`served`) marks such a command unserved with "a request
that is not a JSON body" whenever `ess_gen::http::request(bound).encoding != Encoding::Json`, so
every request for it is answered `501`.

## Acceptance

- `a_form_encoded_command_runs` — a command bound with `encoding: form`, sent
  `title=Dune&year=1965`: it runs and the view lists the row with `year` as an integer.
- `a_query_encoded_command_runs` — a command bound with `encoding: query` on `delete` or `get`,
  sent its input in the query string: it runs.
- `a_form_field_the_command_does_not_declare_is_refused_400` — refused, naming the field, nothing
  written.
- `a_form_field_carried_in_the_path_too_is_refused_400` — as for a JSON body today.
- `a_json_body_to_a_form_route_is_refused` — a body whose content type is not the declared
  encoding is refused `400`, nothing written, as ESS's served application refuses it.

## Origin

Requested by a consumer that serves a specification over HTTP in memory as a test backend for a
browser UI suite, 2026-11-06.

## Scope

- `crates/spindle-ingress/src/surface.rs` — cited: the "a request that is not a JSON body" refusal
  in `served`.
- `crates/spindle-ingress/src/answer.rs` — cited: `input`, which reads only a JSON object.
- `crates/spindle-ingress/src/listener.rs` — inferred: passing the query string and content type
  to the answer.

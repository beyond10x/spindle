---
format: aep.planning-md/3
id: story:query-input-beside-body
kind: story
status: draft
title: A command route reads input fields from the query string beside its body
revision: 1
---
# A command route reads input fields from the query string beside its body

## Outcome

A command whose `ess-http/1` binding reads some input fields from the query string and the rest from
the body (`request.query` not empty) is served: each query field is read as ESS 0.99.1 publishes it,
joined with the body's fields, and the command runs on the joined input as ESS's interpreter runs
it. Today the ingress keeps the route and names it not served, "this ingress does not serve input
fields read from the query string beside the body" (`crates/spindle-ingress/src/surface.rs`,
`served`), and answers it 501.

## Acceptance

- `a_command_reads_its_query_fields_beside_the_body`: a binding with one query field and one body
  field; a request carrying both answers what the interpreter answers for the joined input.
- `a_missing_required_query_field_is_refused_as_ess_publishes_it`: the status and body ESS's binding
  gives a request without it.
- `a_query_field_that_is_not_a_scalar_is_still_not_served`: named at start, as today for a view
  parameter.

## Origin

A consumer, 2026-11-26, on Spindle 0.21.0: a served run of its specification reported this reason for
`POST /api/v3/tasks/template/{template}`; no journey reached the route.

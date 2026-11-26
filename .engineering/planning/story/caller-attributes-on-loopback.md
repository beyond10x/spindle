---
format: aep.planning-md/3
id: story:caller-attributes-on-loopback
kind: story
status: draft
title: A loopback served run serves a command that reads its caller
revision: 1
---
# A loopback served run serves a command that reads its caller

## Outcome

A command that reads caller attributes is served on a loopback run where no caller is fixed. Today
the ingress names it not served, "this ingress does not serve the caller attributes it reads
(`<attribute>`), which no HTTP caller proves here" (`crates/spindle-ingress/src/surface.rs`, `served`),
unless the run fixes a caller (`--fixed-caller`) or is served behind a proxy that proves each caller
(`Surface::proving_callers`, the caller header with the proxy secret). The story decides, when
scoped, which of these holds and delivers it:

- each request on a loopback run may prove its caller in the caller header, without a proxy secret,
  because only local processes reach the listener; or
- the existing routes already serve it, and the refusal names the flag or setting that does.

## Acceptance

- `a_loopback_request_proving_its_caller_runs_the_command`: a command guarded on a caller attribute
  answers as ESS's interpreter answers for that caller; or, for the second branch,
  `the_refusal_names_how_to_serve_a_caller_reading_command`.
- `an_exposed_run_still_requires_the_proxy_secret`: unchanged.

## Origin

A consumer, 2026-11-26, on Spindle 0.21.0: a served run of its specification reported this reason for
`POST /api/v1/kv` (attribute `account_id`); no journey reached the route.

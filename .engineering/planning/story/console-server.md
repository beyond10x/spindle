---
format: aep.planning-md/3
id: story:console-server
kind: story
status: draft
title: The observability console serves its pages and forwards read-only queries
tags:
- needs-timo-acceptance
relations:
- serves: vision:spindle
- decomposes: epic:observability-console
- depends_on: story:console-pages-specified
scope:
- confidence: cited
  path: Cargo.toml
- confidence: cited
  path: Taskfile.yml
- confidence: inferred
  path: crates/spindle-console
- confidence: inferred
  path: ui/console.html
- confidence: cited
  path: ui/package.json
- confidence: inferred
  path: ui/src/console/main.ts
- confidence: inferred
  path: ui/vite.console.config.ts
revision: 4
---
# The observability console serves its pages and forwards read-only queries

**Needs Timo's acceptance**: a second shipped application beside `b10x-spindle`. The epic,
`epic:observability-console`, says why it is not the embedded panel grown up.

## Outcome

A new binary, `b10x-spindle-console`, in a new crate, started with `--prometheus <url>` and one or
more `--model <url>`, listens on one port and:

- serves the console's bundle, embedded at build time (a third Vite build in `ui/`,
  `vite.console.config.ts`, into `ui/dist-console/`);
- forwards `GET` only, and only to the configured URLs: Prometheus's `/api/v1/query`,
  `/api/v1/query_range` and `/api/v1/series`, and each model's `/api/model`, `/__spindle/topology`,
  `/__spindle/metrics/families`, `/__spindle/requests` and `/__spindle/traces/{id}`;
- keeps no state, needs no writable directory, and runs as a non-root user.

Any other method or path answers `404` or `405`; the console never forwards to a URL it was not
started with.

## Acceptance

- `the_console_serves_its_bundle`: `GET /` answers the embedded page.
- `queries_reach_the_configured_prometheus`: a `query_range` request reaches a test Prometheus
  with its parameters unchanged and its answer is returned unchanged.
- `an_unconfigured_path_answers_404`: a request for any path outside the list above answers `404`.
- `an_unconfigured_path_makes_no_outbound_request`: the test Prometheus and the test models
  receive nothing for that request.
- `a_post_answers_405`: a `POST` to a forwarded path answers `405`.
- `two_models_are_forwarded_apart`: with two `--model` URLs, each model's routes reach only its own
  URL.
- `the_console_writes_nothing`: run with a read-only root directory, every route above answers.

## Scope

- `crates/spindle-console/` (new crate: binary `b10x-spindle-console`, `build.rs` embedding
  `ui/dist-console`, as `crates/spindle-cli/build.rs` embeds the panel); `Cargo.toml` workspace
  members.
- `ui/vite.console.config.ts` (new), `ui/console.html` (new), `ui/src/console/main.ts` (new),
  `ui/package.json`.
- `Taskfile.yml` (`task ui` builds `ui/dist-console`).

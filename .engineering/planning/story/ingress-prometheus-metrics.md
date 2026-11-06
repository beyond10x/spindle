---
format: aep.planning-md/3
id: story:ingress-prometheus-metrics
kind: story
status: active
title: An observed run exposes its metrics as Prometheus text
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability
- depends_on: story:ingress-request-log
scope:
- confidence: inferred
  path: crates/spindle-cli/Cargo.toml
- confidence: inferred
  path: crates/spindle-cli/tests/observe.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-ingress/src/console.rs
- confidence: inferred
  path: crates/spindle-ingress/src/observe.rs
- confidence: inferred
  path: crates/spindle-ingress/src/observe/metrics.rs
- confidence: cited
  path: crates/spindle-telemetry/src/counters.rs
revision: 11
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T14:45:46Z", actor: "human:timo", revision: 10, decided_on: {"recorded":{"review_outcome":1}}}
- {from: "proposed", to: "active", at: "2026-11-06T14:45:47Z", actor: "human:timo", revision: 11, decided_on: {"recorded":{"review_outcome":1}}}
---
# An observed run exposes its metrics as Prometheus text

## Outcome

With `--observe`, `GET /__spindle/metrics` answers `text/plain; version=0.0.4` with exactly the
families of `ess/fixtures/observe/metrics/families.json`:

- From the request recorder: `spindle_http_requests_total` and
  `spindle_http_request_duration_seconds`. Route labels are templates, so cardinality is bounded
  by the specification; an unrouted request has `route=""`. View reads are
  `spindle_http_requests_total{kind="view"}`; there is no separate view family.
- From the `Counters` the host already holds per component (`Host::counters`), read at scrape
  time: commands by outcome and failure, command duration, events appended, bindings, binding
  retries, timer ticks, store rows and bytes written. Histogram bounds are `DURATION_BOUNDS_US` in
  seconds.
- `spindle_entity_rows`, a gauge read from each component's store when scraped.
- `spindle_resets_total`, counted where `console::reset` empties the stores.

A reset (`POST /api/reset`) leaves every counter and the request log intact. Counters stay
monotonic, as Prometheus expects. Only `spindle_entity_rows` falls, because it reads the emptied
stores, and `spindle_resets_total` rises by one.

The request goes through an `Exchange` to the host's thread, as a console route does, because rows
are read from the stores. `spindle_external_causes_total` is declared here with no samples;
`story:external-answers-observed` fills it.

## Acceptance

- `the_metrics_parse_as_prometheus_text`: the answer parses with a Prometheus text parser
  (dev-dependency), and every line is `# HELP`, `# TYPE` or a sample of a declared family.
- `the_metrics_count_a_scripted_request_sequence`: after the sequence of
  `the_request_log_lists_a_scripted_sequence_newest_first`, the samples hold, among others:
  `spindle_http_requests_total{method="POST",route="/tasks/{task_id}/close",operation="tasks.board.CloseTask",kind="command",class="served",status_class="2xx"} 1`,
  the same with `status_class="4xx"` 1, the `unrouted` samples summing to 2,
  `spindle_http_requests_total{method="GET",route="/tasks",operation="tasks.board.AllTasks",kind="view",class="served",status_class="2xx"} 1`,
  `spindle_commands_total{command="tasks.board.CloseTask",outcome="closed"} 1`, and a
  `spindle_http_request_duration_seconds_count` of 7.
- `the_exposed_families_are_the_specified_families`: the `# TYPE` lines equal
  `families.json` in name, kind and order.
- `every_catalogue_counter_is_exposed`: each counter of `spindle_telemetry::catalogue` for each
  served component maps to one sample.
- `entity_rows_follow_writes_and_resets`: two opened tasks give `spindle_entity_rows` 2; a reset
  gives 0 and `spindle_resets_total` 1.
- `a_reset_leaves_counters_and_the_log_intact`: after the scripted sequence and a reset, every
  counter and histogram sample except `spindle_entity_rows` and `spindle_resets_total` equals
  what it was before the reset. `GET /__spindle/requests` keeps the same `recorded`, `totals` and
  records, plus the reset's own record. A request after the reset adds to the earlier counts.
- `reading_the_metrics_changes_no_count`: two scrapes in a row are equal.

## Scope

- `crates/spindle-ingress/src/observe.rs` — inferred: the `metrics` arm of the dispatch (one
  line, shared with `story:ingress-observe-panel` and `story:live-topology`).
- `crates/spindle-ingress/src/observe/metrics.rs` — inferred, new: the text writer.
- `crates/spindle-ingress/src/console.rs` — cited: counting a reset; it clears no counter or
  record.
- `crates/spindle-host/src/host.rs` — cited: a rows-per-entity read through `Store::scan`
  (O(rows) per scrape; a counted gauge is a later optimisation). `Host::reset_to` must leave the
  component `Counters` untouched (inferred; `a_reset_leaves_counters_and_the_log_intact` checks
  it).
- `crates/spindle-telemetry/src/counters.rs` — cited: `Snapshot`, `DURATION_BOUNDS_US`; read only.
- `crates/spindle-cli/tests/observe.rs`, `crates/spindle-cli/Cargo.toml` — inferred: tests, the
  parser as a dev-dependency.

## Order

After `story:ingress-request-log`. Parallel with `story:ingress-observe-panel`. Before
`story:external-answers-observed` and `story:live-topology`.

## Origin

The operator's "mini Prometheus inside", and consumer ask (2) with its acceptance, 2026-11-06,
condensed (see the epic).

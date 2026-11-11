---
format: aep.planning-md/3
id: story:metrics-observed-series
kind: story
status: active
title: A metrics scrape writes only observed series and times each route
relations:
- decomposes: epic:runtime-observability-depth
- serves: vision:spindle
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-11T09:40:08Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-11T09:40:08Z", actor: "human:timo", revision: 3}
---
## Outcome

`GET /__spindle/metrics` writes a series only once something has been counted in it, and a slow
route shows in a request-duration histogram labelled by its route template.

## Why

Reported by a downstream consumer's outside review on 2026-11-11.

- 0.13.0, the consumer's dev deployment: 15,565 series in 14 families, 261 non-zero, 1.5 MB per
  scrape, 7 ms.
- 0.8.0, a second deployment of the same bundle: 13,922 series, 98 % zero.
- Series are declared from the model before anything runs: `spindle_command_duration_seconds`
  is 471 commands × 15 (7,800 on dev), `spindle_command_failures_total` 471 × 10 failure kinds
  with 0 non-zero (5,200 on dev), `spindle_commands_total` every (command, outcome) pair (1,849 on
  dev). About 33 series per declared command; Prometheus holds that once per instance.
- `spindle_http_request_duration_seconds` has no labels (13 buckets, sum, count), and requests
  under `/__spindle/` are not counted. The 21–44 s metrics scrape on 0.8.0 (fixed in 0.12.1 by
  `story:metrics-read-without-scan`) did not show in Spindle's own metrics.

`story:metrics-series-bounded` (draft) aggregates the route and operation labels of request
series; this story removes the never-observed series and adds the route label to the duration
histogram. The two touch the same files.

## Acceptance

1. On a served run of a specification of at least 400 commands with nothing executed, a scrape
   writes fewer than 1,000 series; the test states the number.
2. A (command, outcome), (command, failure kind), event type, binding or entity series appears in
   the scrape only after it was counted once, and stays after that.
3. `spindle_http_request_duration_seconds` carries the route template (or `kind`) as a label; a
   view read held over 1 s shows in the `+Inf`/`le="1"` difference of that route's series only.
4. A gauge reports the duration of the last `/__spindle/metrics` scrape.
5. Optional: a gauge per store reports the event-log length.
6. `website/docs/concepts/observe.md` states which families are written only once observed.

## Scope

- `crates/spindle-ingress/src/observe.rs`, inferred
- `crates/spindle-ingress/src/observe/metrics.rs`, inferred
- `crates/spindle-cli/src/serve.rs`, inferred
- `website/docs/concepts/observe.md`, inferred

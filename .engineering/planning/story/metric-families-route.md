---
format: aep.planning-md/3
id: story:metric-families-route
kind: story
status: draft
title: An observed run describes its metric families at /__spindle/metrics/families
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability-depth
- depends_on: story:observe-label-vocabulary
scope:
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/observe.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe/metrics.rs
revision: 2
---
# An observed run describes its metric families at /__spindle/metrics/families

## Outcome

With `--observe`, `GET /__spindle/metrics/families` answers a `spindle.observe.MetricFamilies`:
every family `GET /__spindle/metrics` exposes, with its kind, labels, level, unit and summary, in
the exposition's order. A reader builds its queries from this answer instead of from a list it was
given. Without `--observe` it answers `404`, like every reserved route.

## Acceptance

- `the_families_route_lists_what_the_metrics_expose`: on the rally example served by `run`, every
  family named by a `# TYPE` line of `/__spindle/metrics` is in the answer with the same kind, and
  nothing else is.
- `the_families_answer_reads_as_its_specified_type`: the answer deserialises into
  `spindle_observe_api::MetricFamilies` and validates against its schema.
- `without_observe_the_families_route_answers_404`.
- A request to the route is neither recorded nor counted, as for every reserved route.

## Scope

- `crates/spindle-ingress/src/observe.rs` (`Reserved`, lines 56-68; `reserved`, line 642): one more
  arm.
- `crates/spindle-ingress/src/observe/metrics.rs` (`FAMILIES`).
- `crates/spindle-cli/tests/` (the observe tests).

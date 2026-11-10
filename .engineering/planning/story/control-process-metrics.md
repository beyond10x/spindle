---
format: aep.planning-md/3
id: story:control-process-metrics
kind: story
status: draft
title: The control process of deploy serves the deployment's samples as Prometheus text
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability-depth
- depends_on: story:observe-label-vocabulary
- depends_on: story:metric-families-route
scope:
- confidence: cited
  path: crates/spindle-control/src/hub.rs
- confidence: cited
  path: crates/spindle-control/src/server.rs
- confidence: cited
  path: crates/spindle-control/tests/spindle-control
- confidence: cited
  path: crates/spindle-ingress/src/observe/metrics.rs
revision: 3
---
# The control process of deploy serves the deployment's samples as Prometheus text

## Outcome

The control process `deploy` runs answers `GET /__spindle/metrics` with the samples it already
collects from every instance (`spindle-telemetry/1` frames), as Prometheus text with the same
family names and the vocabulary of `story:observe-label-vocabulary`, plus an `instance` label: the
placement's instance id, which the operator declares and the placement bounds, never an entity
identity. `GET /__spindle/metrics/families` describes them. Both are guarded by the control
process's token and `Host` check, like its API. This is the "later story" of
`docs/design/observe.md` § 7.

## Acceptance

- `the_control_process_has_a_series_per_instance`: with the rally placement deployed, every family
  has one series per placed instance.
- `the_control_counts_equal_the_frames`: each command counter of an instance equals the sum its
  frames reported.
- `the_control_metrics_need_the_token`: without the token the route answers `401`.
- `the_control_metrics_check_the_host`: with the token and a foreign `Host` the route answers `403`.
- `the_control_families_match_its_exposition`: every family named by a `# TYPE` line of the control
  process's `/__spindle/metrics` is listed by its `/__spindle/metrics/families` with the same kind,
  and the description lists no other family.

## Scope

- `crates/spindle-control/src/server.rs` (routes, lines 239-262), `crates/spindle-control/src/hub.rs`
  (the collected samples).
- `crates/spindle-ingress/src/observe/metrics.rs` (the text writer, shared so both use it).
- `crates/spindle-control/tests/`.

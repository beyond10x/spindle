---
format: aep.planning-md/3
id: story:store-and-kernel-durations
kind: story
status: draft
title: An observed run times the ingress, the decision and the store separately
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability-depth
- depends_on: story:observe-label-vocabulary
- depends_on: story:metric-families-route
scope:
- confidence: cited
  path: crates/spindle-host/src/admission.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe/metrics.rs
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-telemetry/src/catalogue.rs
- confidence: cited
  path: crates/spindle-telemetry/src/counters.rs
- confidence: cited
  path: crates/spindle-telemetry/src/kernel.rs
- confidence: cited
  path: crates/spindle-telemetry/src/port.rs
- confidence: cited
  path: ess/fixtures/observe/metrics/families.json
revision: 3
---
# An observed run times the ingress, the decision and the store separately

## Outcome

The single command histogram (`spindle_command_duration_seconds`, `metrics.rs:74-78`, measured
around the whole of `execute_refusing`, `host.rs:1339-1358`) is split by level, each a histogram
with `_sum` and `_count`, labelled with the vocabulary of `story:observe-label-vocabulary`:

- ingress: the request histogram gains `domain`, `kind` and `name` of the operation it routed
  (today it has no labels, `metrics.rs:55-59`);
- command: the kernel's decision (guards, outcome selection, effects) per command, store time
  excluded;
- store: one read and one append per component and entity, timed in `CountingStore`;
- admission: one gauge per component, how long its admission took at start.

The existing `spindle_command_duration_seconds` stays, so no reader breaks. Each timing is also
emitted as a telemetry `Record` with its start instant, which `story:request-span-tree` reads.

## Acceptance

- `store_reads_and_appends_are_timed`: after a scripted sequence on the rally example, the store
  read and append histograms' `_count` per entity equal the reads and appends the sequence made.
- `the_decision_excludes_the_store`: with a store that sleeps a fixed time per append, the
  command decision histogram's `_sum` stays below that time while the append histogram's grows
  by it.
- `the_request_histogram_names_the_routed_operation`: each sample of a routed request carries the
  routed command's or view's `name`.
- `an_unrouted_request_has_an_empty_name`: a request to no route is counted with an empty `name`.
- `admission_is_timed_once_per_component`: the admission gauge has one sample per component and
  does not move after start.
- `/__spindle/metrics/families` lists the new families with their levels.

## Scope

- `crates/spindle-telemetry/src/kernel.rs` (`CountingStore`, `Sink::executed`),
  `src/catalogue.rs` (`Key`, `Kind`), `src/counters.rs` (`DURATION_BOUNDS_US`, sums),
  `src/port.rs` (`Record`).
- `crates/spindle-host/src/host.rs` (the execution at lines 1330-1362),
  `crates/spindle-host/src/admission.rs`.
- `crates/spindle-kernel/src/execute.rs` (where the decision ends and the commit starts).
- `crates/spindle-ingress/src/observe/metrics.rs`, `ess/fixtures/observe/metrics/families.json`.

## Shared files

`story:view-read-families` also edits `crates/spindle-host/src/host.rs`,
`crates/spindle-telemetry/src/catalogue.rs`, `metrics.rs` and `families.json`; it runs after this
story. `story:binding-external-timer-durations` also edits `catalogue.rs`, `metrics.rs` and
`families.json`; it runs after this story.

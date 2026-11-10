---
format: aep.planning-md/3
id: story:view-read-families
kind: story
status: draft
title: An observed run counts and times every view read, with the rows it returned
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability-depth
- depends_on: story:observe-label-vocabulary
- depends_on: story:metric-families-route
- depends_on: story:store-and-kernel-durations
scope:
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-host/src/view.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe/metrics.rs
- confidence: cited
  path: crates/spindle-telemetry/src/catalogue.rs
- confidence: cited
  path: crates/spindle-telemetry/src/counters.rs
- confidence: cited
  path: crates/spindle-telemetry/src/port.rs
- confidence: cited
  path: ess/fixtures/observe/metrics/families.json
revision: 3
---
# An observed run counts and times every view read, with the rows it returned

## Outcome

Three families per view, labelled with the vocabulary of `story:observe-label-vocabulary`: reads
by result (answered, refused), the evaluation duration as a histogram, and the rows returned as a
histogram. A one-row read and a list read both count, through `Host::view`, `Host::list` and
`Host::list_with`. Each evaluation is also emitted as a telemetry `Record` with its start instant.

## Acceptance

- `view_reads_are_counted_by_view`: after reading `MatchState` three times and a list view twice
  on the rally example, the reads family has those counts under each view's `name`.
- `rows_returned_follow_the_answer`: the rows histogram's `_sum` equals the total number of rows
  the scripted list reads answered.
- `a_refused_view_read_is_counted_as_refused`: a read with a required parameter left out counts
  under `refused`.
- `a_refused_view_read_adds_no_rows`: that read adds no sample to the rows histogram.
- `/__spindle/metrics/families` lists the three families at level `view`.

## Scope

- `crates/spindle-host/src/host.rs` (`view` line 1217, `list` line 1253, `list_with` line 1288),
  `crates/spindle-host/src/view.rs`.
- `crates/spindle-telemetry/src/catalogue.rs`, `src/counters.rs`, `src/port.rs` (`Record`).
- `crates/spindle-ingress/src/observe/metrics.rs`, `ess/fixtures/observe/metrics/families.json`.

## Shared files

`story:store-and-kernel-durations` edits `crates/spindle-host/src/host.rs`,
`crates/spindle-telemetry/src/catalogue.rs`, `metrics.rs` and `families.json` too; this story runs
after it.

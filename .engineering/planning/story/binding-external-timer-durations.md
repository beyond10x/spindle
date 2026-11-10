---
format: aep.planning-md/3
id: story:binding-external-timer-durations
kind: story
status: draft
title: An observed run times binding delivery, external answers and tick lag
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability-depth
- depends_on: story:observe-label-vocabulary
- depends_on: story:store-and-kernel-durations
- depends_on: story:metric-families-route
scope:
- confidence: cited
  path: crates/spindle-channel/src/endpoint.rs
- confidence: cited
  path: crates/spindle-channel/src/model.rs
- confidence: cited
  path: crates/spindle-cli/src/decisions.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-host/src/host/component.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe/metrics.rs
- confidence: cited
  path: crates/spindle-telemetry/src/catalogue.rs
- confidence: cited
  path: crates/spindle-telemetry/src/port.rs
- confidence: cited
  path: crates/spindle-timer/src/lib.rs
- confidence: cited
  path: ess/fixtures/observe/metrics/families.json
revision: 3
---
# An observed run times binding delivery, external answers and tick lag

## Outcome

Three more histograms, labelled with the vocabulary of `story:observe-label-vocabulary`:

- binding delivery: from an event's envelope leaving the outbox to its command having run at the
  receiving component, per binding; over `tcp` the sending instant travels in the envelope;
- external answer: how long the decider took to answer an external cause, per command and source
  (world, file, default). An external cause is answered in the process by a decider
  (`crates/spindle-host/src/decisions.rs:1`), not by a network call, so this is the decider's time;
- tick lag: from a periodic tick's deadline to its command starting, per binding.

Each timing is also emitted as a telemetry `Record` with its start instant.

## Acceptance

- `binding_delivery_is_timed_per_binding`: after the rally match is opened and ticked, each event
  binding's delivery histogram `_count` equals its `spindle_bindings_total{carried="delivered"}`.
- `external_answers_are_timed_by_source`: with a decisions file answering one cause and the
  default another, the histogram has one series per source with the matching counts of
  `spindle_external_causes_total`.
- `tick_lag_reads_the_injected_clock`: with the injected clock advanced 250 ms past a deadline
  before `Host::advance`, the lag sample falls in the bucket holding 0.25 s.
- `/__spindle/metrics/families` lists the three at levels `binding`, `external` and `timer`.

## Scope

- `crates/spindle-channel/src/endpoint.rs` (`Sender`, `Receiver`), `src/model.rs` (`Envelope`:
  the sending instant).
- `crates/spindle-host/src/host.rs` (`advance`, line 1167), `src/host/component.rs` (`deliver`,
  line 719), `crates/spindle-cli/src/decisions.rs` (the instrumented decider).
- `crates/spindle-timer/src/lib.rs` (`Tick`, the deadline).
- `crates/spindle-telemetry/src/catalogue.rs`, `src/port.rs` (`Record`);
  `crates/spindle-ingress/src/observe/metrics.rs`; `ess/fixtures/observe/metrics/families.json`.

## Shared files

Runs after `story:store-and-kernel-durations` (`catalogue.rs`, `port.rs`, `host.rs`, `metrics.rs`,
`families.json`). `story:queue-families` edits `crates/spindle-channel/src/endpoint.rs`,
`crates/spindle-timer/src/lib.rs`, `metrics.rs` and `families.json` too, and runs after this story.

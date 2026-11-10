---
format: aep.planning-md/3
id: story:queue-families
kind: story
status: draft
title: 'An observed run exposes what waits: outbox, unacknowledged envelopes, pending ticks'
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability-depth
- depends_on: story:observe-label-vocabulary
- depends_on: story:metric-families-route
- depends_on: story:binding-external-timer-durations
scope:
- confidence: cited
  path: crates/spindle-channel/src/endpoint.rs
- confidence: cited
  path: crates/spindle-channel/src/outbox.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe/metrics.rs
- confidence: cited
  path: crates/spindle-timer/src/lib.rs
- confidence: cited
  path: ess/fixtures/observe/metrics/families.json
revision: 3
---
# An observed run exposes what waits: outbox, unacknowledged envelopes, pending ticks

## Outcome

Gauges read at scrape time, labelled with the vocabulary of `story:observe-label-vocabulary`:

- outbox depth per component and the age of its oldest envelope (`Sender::staged` counts them
  today and nothing exposes it, `crates/spindle-channel/src/endpoint.rs:295-298`);
- envelopes sent and not yet acknowledged, per `at_least_once` binding;
- ticks pending per periodic binding (the one pending tick `one_pending_drop_excess` keeps,
  `crates/spindle-timer/src/lib.rs:30`).

## Acceptance

- `the_outbox_depth_is_what_waits`: with a receiving component that does not acknowledge, after
  three events the outbox depth plus the unacknowledged gauge for that binding equals three.
- `acknowledged_envelopes_leave_the_gauges`: once the receiver acknowledges, both gauges for that
  binding are zero.
- `the_oldest_envelope_ages_on_the_clock`: the age gauge equals the injected clock's distance to
  the oldest staged envelope.
- `a_pending_tick_is_shown`: with a tick open and its next deadline passed, the pending gauge for
  that binding is 1.
- `/__spindle/metrics/families` lists the gauges at level `queue`.

## Scope

- `crates/spindle-channel/src/endpoint.rs` (`staged`, the retry and acknowledgement state),
  `src/outbox.rs` (staging instant).
- `crates/spindle-timer/src/lib.rs` (pending ticks).
- `crates/spindle-host/src/host.rs` (reading them at scrape time).
- `crates/spindle-ingress/src/observe/metrics.rs`, `ess/fixtures/observe/metrics/families.json`.

## Shared files

`story:binding-external-timer-durations` edits `crates/spindle-channel/src/endpoint.rs`,
`crates/spindle-timer/src/lib.rs`, `crates/spindle-host/src/host.rs`, `metrics.rs` and
`families.json` too; this story runs after it.

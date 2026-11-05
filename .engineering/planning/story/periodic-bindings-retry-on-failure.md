---
format: aep.planning-md/3
id: story:periodic-bindings-retry-on-failure
kind: story
status: draft
title: 'Periodic bindings that declare on_failure: retry are retried or refused'
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-host/tests/retry.rs
revision: 3
---
# Periodic bindings that declare on_failure: retry are retried or refused

## Outcome

A periodic binding that declares `on_failure: retry` is either retried as an event binding is, or
refused at startup naming the binding. Today it is neither: `uncarried` and `retried` in
`crates/spindle-host/src/host.rs` look only at event bindings, so the declared policy is silently
ignored.

## Acceptance

- `a_periodic_retry_binding_is_retried_or_refused` — a periodic binding with `on_failure: retry`
  whose target refuses twice and then succeeds: it is attempted three times, or the host refuses
  to start and names the binding (the chosen behaviour stated in the test and the design).
- `retried_lists_periodic_retry_bindings` — the startup list and `--drop-retries` cover it too.

## Origin

Pre-existing. Found by the adversary on `story:event-bindings-retry-on-failure` (wave 6).

## Scope

- `crates/spindle-host/src/host.rs` — cited: `uncarried`, `retried`, the periodic path.
- `crates/spindle-host/tests/retry.rs` — inferred: the tests.

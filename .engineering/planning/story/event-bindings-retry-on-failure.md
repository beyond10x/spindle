---
format: aep.planning-md/3
id: story:event-bindings-retry-on-failure
kind: story
status: implemented
title: Event bindings retry on failure
relations:
- serves: vision:spindle
- decomposes: epic:m0-run-in-process
scope:
- confidence: inferred
  path: crates/spindle-channel
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-host/tests/retry.rs
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-05T09:39:06Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-05T09:39:06Z", actor: "human:timo", revision: 5}
- {from: "active", to: "implemented", at: "2026-11-05T22:03:27Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":2}}}
---
# Event bindings retry on failure

## Outcome

The host runs an event binding that declares `on_failure: retry`: when the invoked command fails
(refused or erred), the invocation is retried until it succeeds or a bound retry limit is reached,
and each attempt is counted. Today `Host::new` refuses every such binding
(`crates/spindle-host/src/host.rs:253`), so a specification that declares one cannot run at all,
and `run --scenario` replays nothing.

Requested by a downstream specification probe (2026-11-05): all 14 of its bindings declare it.
Reproduced with a neutral fixture.

## Acceptance

- `a_failing_invocation_is_retried_until_it_succeeds` — a binding whose target refuses twice and
  then succeeds runs it three times; the state reflects the success.
- `retries_stop_at_the_bound_limit` — a target that always refuses is tried exactly the bound
  number of times and the failure is recorded, not lost.
- `a_retry_does_not_reorder_later_events` — events published after the failing one are invoked
  in order once the retry settles (or the chosen ordering is stated in the design and tested).
- `retry_attempts_are_counted` — the telemetry counters record attempts and final failures.

## Scope

- `crates/spindle-host/src/host.rs` — cited: the refusal at line 253.
- `crates/spindle-channel` — inferred: the redelivery path.
- `crates/spindle-host/tests/retry.rs` — inferred, new.

ESS semantics of `on_failure: retry` and `delivery:` read from ess 0.86.0; the retry limit and
backoff are runtime defaults (design §11 rule: runtime defaults first), stated in the design.

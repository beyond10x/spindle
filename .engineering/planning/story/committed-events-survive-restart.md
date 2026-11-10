---
format: aep.planning-md/3
id: story:committed-events-survive-restart
kind: story
status: draft
title: Events committed before a crash are delivered after the restart
relations:
- serves: vision:spindle
- decomposes: epic:split-run
- depends_on: story:run-one-component
- depends_on: story:events-read-from-store
scope:
- confidence: cited
  path: crates/spindle-channel/src/endpoint.rs
- confidence: cited
  path: crates/spindle-channel/src/outbox.rs
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-host/src/host/component.rs
- confidence: cited
  path: website/docs/concepts
revision: 3
---
# Events committed before a crash are delivered after the restart

## Outcome

The outbox is memory, and its documentation leaves crash recovery to the producer's log, which
"replays the events of every durable position that was not yet sent"
(`crates/spindle-channel/src/outbox.rs:8-9`); nothing replays today. A component process with a
durable store records, per binding, the last store position whose envelopes were sent (and, for
`at_least_once`, acknowledged). On start it reads the committed events after that position through
the `Store` read `story:events-read-from-store` adds, and stages them again, so an `at_least_once`
binding delivers each one after a crash, and an `at_most_once` binding delivers it at most once.

Pending periodic ticks are not persisted: ESS's periodic profile declares `lifetime:
host_instance`, "a new host instance is a new activation" (`crates/spindle-timer/src/lib.rs:31`).
The command-line help and the website say so.

## Acceptance

- `an_unsent_event_is_delivered_after_restart`: with a sender killed (SIGKILL) after committing an
  event bound `at_least_once` and before the receiver acknowledged it, the restarted sender
  delivers it and the receiver runs the bound command once.
- `a_sent_event_is_not_sent_twice_at_most_once`: for an `at_most_once` binding whose envelope was
  sent before the kill, the restarted sender does not send it again.
- `ticks_restart_as_a_new_activation`: the help of `run` states that a restart starts each periodic
  binding anew, as the profile says.

## Scope

- `crates/spindle-channel/src/endpoint.rs` (`Sender`, `flush`), `crates/spindle-channel/src/outbox.rs`.
- `crates/spindle-host/src/host/component.rs` (start of a component: the replay).
- `crates/spindle-cli/src/cli.rs` (help), `website/docs/concepts/` (the limit).

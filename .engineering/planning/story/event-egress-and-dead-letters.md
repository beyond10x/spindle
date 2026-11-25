---
format: aep.planning-md/3
id: story:event-egress-and-dead-letters
kind: story
status: draft
title: Committed events leave to a broker or webhook, and exhausted commands land as dead letters
summary: Events bound by the environment are delivered at least once to a broker or webhook; a binding command that exhausts its retries is kept and replayable.
tags:
- needs-timo-acceptance
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- informed_by: story:committed-events-survive-restart
revision: 1
---
# Committed events leave to a broker or webhook, and exhausted commands land as dead letters

## Outcome

An environment binds a specification's published events to a broker or a webhook, and Spindle
delivers each committed event there at least once, in commit order per stream. A binding's command
that exhausts its retries is kept as a dead letter an operator can list and replay, not only
printed.

## Why

- No event leaves the process except over Spindle's own channels between components: no broker
  or webhook delivery exists in `crates/`.
- A binding's command that fails after its retries is printed on stderr as "dropped" and lost
  (`crates/spindle-cli/src/serve.rs:456-471`).
- A consumer's service has other systems that react to its events.

## Acceptance

- `a_committed_event_reaches_the_webhook`: with an event bound to a webhook, a committed event is
  posted once; with the webhook down at commit, it is posted after it comes back, also across a
  restart (`story:committed-events-survive-restart`).
- `an_exhausted_command_is_kept_as_a_dead_letter`: a binding command refused on every retry is
  listed with its event, command, error and attempts, survives a restart, and a replay runs it
  once.

## Model first

A dead letter is a noun Spindle's own specification does not declare. It is declared in `ess/`
(`spindle.observe` or a domain of its own) and validated before this story is proposed. The
environment names the broker or webhook; a specification names neither (AGENTS.md).

## Decided when scoped

- Which brokers, the capability key the environment binds, and whether egress and dead letters
  are two stories.

## Needs Timo's acceptance

The delivery is an outbound connection a run makes on its own.

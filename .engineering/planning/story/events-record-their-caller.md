---
format: aep.planning-md/3
id: story:events-record-their-caller
kind: story
status: draft
title: Every committed event records the caller that caused it
summary: Each committed event records the actor, caller attributes and request id of its cause instead of spindle-kernel.
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- informed_by: story:http-caller-proves-actor
revision: 1
---
# Every committed event records the caller that caused it

## Outcome

Each event a store commits records the actor and caller attributes of the request, seed entry or
binding that caused it, and the request id, so an auditor reads who did what from the store
alone.

## Why

Every append records the runtime as its subject and actor: `meta` sets both to `ACTOR`,
`"spindle-kernel"` (`crates/spindle-kernel/src/sqlite.rs:94-95`, `sqlite.rs:2200-2210`), and a fresh
request id, whatever the request carried. The PostgreSQL store appends through the same log
(`crates/spindle-kernel/src/postgres.rs:39`). `Origin`, what a read of committed events answers,
holds the command and its sequence only (`crates/spindle-kernel/src/store.rs:143-150`).

## Acceptance

- `an_event_records_the_caller_of_its_request`: a command sent by a proven caller commits events
  whose metadata name that actor and its attributes, read back through `Store::events_after` on
  SQLite and PostgreSQL.
- `an_event_a_binding_caused_records_its_cause`: an event a binding's command committed names the
  binding and the caller of the event that triggered it.
- `a_seed_event_records_the_seed`: an event a seed entry committed names the seed and the entry's
  actor.

## Decided when scoped

- How stores written before this change read back (no caller recorded), and whether
  `GET /api/events` answers the caller.

---
format: aep.planning-md/3
id: story:entity-event-history
kind: story
status: draft
title: A consumer reads the committed events of one entity instance
relations:
- decomposes: epic:split-run
- serves: vision:spindle
- informed_by: story:events-read-from-store
revision: 1
---
# A consumer reads the committed events of one entity instance

## Outcome

A consumer reads the committed events of one entity instance (component, entity, identity), oldest
first, with position, time, command, caller and outcome, paged; used to show the revision history
of a record that commands change in place.

## Why

Asked for by a downstream consumer on 2026-11-10, in its words above. Today `GET /api/events`
answers the events the host published in this process, with no filter
(`crates/spindle-ingress/src/console.rs:84-86`). `story:events-read-from-store` (draft) adds one
store read of all committed events after a position, in commit order, and has no per-instance
filter. No priority over wave 19 was asked for.

## Acceptance

- After three commands change one instance and one command changes another, a read of the first
  instance answers its three events, oldest first, each with its position, time, command, caller
  and outcome, and none of the other instance's.
- A read with a page size of 2 answers two events and a cursor; the next page answers the third.
- A read of an instance no command changed answers an empty list.

## Open

- Where the read is served (a reserved route or the console API) and whether caller and outcome are
  stored beside each event today: not established.

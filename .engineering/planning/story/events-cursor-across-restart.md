---
format: aep.planning-md/3
id: story:events-cursor-across-restart
kind: story
status: draft
title: An events cursor reads the events committed after a restart
relations:
- serves: vision:spindle
- informed_by: story:entity-event-history
revision: 1
---
## Outcome

A `GET /api/events` cursor taken before a restart reads every event committed after the restart,
on any clock. Today `range::range` (`crates/spindle-host/src/host/range.rs`) starts a cursor from
another run at position 0 and drops events by a key built on the command sequence; a restarted host
numbers its commands from 0 again, and on a fixed clock (`run --clock <at>` with `--state`) the key
of a new event equals one the cursor already passed, so the event is skipped without an error.

## Why

Found in wave 28 by the adversary of `story:entity-event-history`, whose history cursor had the same
flaw and was corrected to resume a cursor from another run by store position. `/api/events` was not
changed in that story. Present before wave 28.

## Acceptance

- `an_events_cursor_from_before_a_restart_reads_the_events_after_it`: on SQLite with a fixed clock,
  a cursor taken before a restart reads, after the restart, every event the restarted host commits,
  once each and in order.
- A cursor from the same run reads as today: the existing `/api/events` tests stay green.

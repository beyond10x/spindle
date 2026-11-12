---
format: aep.planning-md/3
id: story:events-head-cursor
kind: story
status: implemented
title: GET /api/events answers the newest cursor without reading the log
relations:
- serves: vision:spindle
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-11T20:31:12Z", actor: "human:timo", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-11T20:32:22Z", actor: "human:timo", revision: 4}
- {from: "active", to: "implemented", at: "2026-11-11T23:08:10Z", actor: "human:timo", revision: 5, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

A client that wants only events from now on gets the newest cursor of `GET /api/events` in one
request that reads no event of the log, and polls after it.

## Why

A downstream consumer on 0.14.0 has about 179,000 events on its dev state. `story:events-range-read`
answers a page after a cursor, but there is no request that answers the cursor at the end:
`limit=0` is refused `400`, and a bare `GET /api/events` answers the whole list without `next`.
Reaching the end means paging from the start, 179 pages at `limit=1000`, and until
`story:events-read-bounded` each page's store read reads the rest of the log.

Measured by the same consumer on a copy of its state on 0.14.0: 63 pages of 1,000 events, 19.7 MB,
79 s; page 1 took 4.39 s and page 50 0.32 s, so a page costs the remaining feed and a catch-up is
quadratic in the log. A poll at the end takes 0.5 ms and answers 150 bytes. Asked for:
`GET /api/events?after=head` or `limit=0` answering only `next`.

## Acceptance

1. A request on `GET /api/events` (form to be decided: a reserved `after` value or a parameter)
   answers `{"events": [], "next": <cursor>}` with the cursor after the last event published, on
   the memory, SQLite and PostgreSQL stores; a following read with that cursor answers the events
   published after it.
2. That request reads no event of a 10,000-event log (a counting test, as
   `events_range::an_idle_read_reads_no_event_of_a_long_log` counts).

## Scope

- `crates/spindle-ingress/src/console.rs`, inferred
- `crates/spindle-host/src/host/range.rs`, inferred
- `crates/spindle-kernel/src/store.rs`, inferred

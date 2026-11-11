---
format: aep.planning-md/3
id: story:events-read-bounded
kind: story
status: active
title: A page of /api/events reads at most its limit from each store
relations:
- serves: vision:spindle
- depends_on: story:events-range-read
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-11T13:36:28Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-11T13:36:28Z", actor: "human:timo", revision: 3}
---
## Outcome

A page of `GET /api/events?after=<cursor>&limit=<n>` reads at most `n` events from each store, so
catching up through a long backlog costs each page its own size, not the rest of the log.

## Why

`story:events-range-read` (wave 21) answers at most `n` events and an idle poll reads none, but the
store read it rests on has no limit: each page reads every event after its position and drops all
but `n`, so paging through 100,000 events re-reads the remaining log on every page.

## Acceptance

1. The store read of committed events after a position takes a limit, on memory, SQLite,
   PostgreSQL and IndexedDB, and the store contract holds it.
2. Paging through 10,000 events with `limit=100` reads at most 100 events per store per page (a
   counting test, as `events_range::an_idle_read_reads_no_event_of_a_long_log` counts).

## Scope

- `crates/spindle-kernel/src/store.rs`, `sqlite.rs`, `postgres.rs`, `contract.rs`, inferred
- `crates/spindle-store-browser/src/lib.rs`, inferred
- `crates/spindle-host/src/host/range.rs`, inferred

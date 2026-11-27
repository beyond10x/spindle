---
format: aep.planning-md/3
id: story:browser-events-cursor-log-id
kind: story
status: draft
title: A browser events cursor reads a recreated database from its start
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

The browser store and the wasm `Shared` forwarder answer an event log id
(`Store::events_id`), so a browser's `GET /api/events` cursor taken before the IndexedDB database
was deleted reads the new database from its start, as a native run's cursor does since wave 30.

## Why

Found by wave 30's U7 adversary: `crates/spindle-wasm/src/worker.rs:305` and the browser store
keep the default log id 0, so a browser cursor falls back to the head check. IndexedDB positions
start over only when the database is deleted, so nothing reaches it today.

## Acceptance

- A wasm test deletes the IndexedDB database between two reads of one cursor and reads every
  event of the new database.

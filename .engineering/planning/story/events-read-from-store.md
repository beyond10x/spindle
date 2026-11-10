---
format: aep.planning-md/3
id: story:events-read-from-store
kind: story
status: draft
title: /api/events answers the events the store holds, so they survive a restart
relations:
- serves: vision:spindle
- decomposes: epic:split-run
- depends_on: story:postgres-store
scope:
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-ingress/src/console.rs
- confidence: cited
  path: crates/spindle-kernel/src/contract.rs
- confidence: inferred
  path: crates/spindle-kernel/src/postgres.rs
- confidence: cited
  path: crates/spindle-kernel/src/sqlite.rs
- confidence: cited
  path: crates/spindle-kernel/src/store.rs
- confidence: cited
  path: crates/spindle-store-browser/src/indexeddb.rs
revision: 4
---
# /api/events answers the events the store holds, so they survive a restart

## Outcome

This story owns the one `Store` read of committed events: the events after a given store position,
in commit order, with each one's position. Every backend implements it (memory, SQLite, PostgreSQL,
IndexedDB) and the store contract holds it.

With it, `GET /api/events` answers from the store instead of from the host's list in memory
(`crates/spindle-host/src/host.rs:431`, `crates/spindle-ingress/src/console.rs:84-86`). Every
committed event is already appended to the store beside the row it changed
(`crates/spindle-kernel/src/sqlite.rs:3-5`), so with `store.durable` bound to `sqlite` or `postgres`
the answer is the same after a restart. With a memory store the answer is what this process
published, as today. `story:committed-events-survive-restart` uses the same read.

## Acceptance

- `the_store_reads_events_after_a_position`: a store contract case: after three commits, reading
  from the first commit's position answers the second and third commits' events in order.
- `events_survive_a_restart`: after three commands on rally bound to `sqlite`, a restart on the same
  state answers the same three events in the same order at `/api/events`.
- `events_keep_the_published_order`: within one process, the answer equals today's
  `Host::published` order for the scripted sequence of the console tests.
- `a_reset_empties_the_events`: after `POST /api/reset`, the answer is empty, as today.

## Scope

- `crates/spindle-kernel/src/store.rs` (`Store`: the read), `crates/spindle-kernel/src/sqlite.rs`,
  `crates/spindle-kernel/src/postgres.rs`, `MemoryStore` (in `store.rs`), `crates/spindle-kernel/src/contract.rs`.
- `crates/spindle-host/src/host.rs` (`published`, line 851).
- `crates/spindle-ingress/src/console.rs` (`events`, line 86).
- `crates/spindle-store-browser/src/` (the same read on IndexedDB).

## Shared files

Runs after `story:postgres-store` (`crates/spindle-kernel/src/sqlite.rs`, the new `postgres.rs`).
`story:committed-events-survive-restart` reads through the primitive and runs after this story.

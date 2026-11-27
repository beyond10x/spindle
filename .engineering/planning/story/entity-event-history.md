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
- depends_on: story:events-record-their-caller
scope:
- confidence: inferred
  path: .engineering/planning/story/console-api.md
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-host/src/host/range.rs
- confidence: cited
  path: crates/spindle-host/src/host/reader.rs
- confidence: inferred
  path: crates/spindle-host/src/host/rows.rs
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/instance_history.rs
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/main.rs
- confidence: cited
  path: crates/spindle-ingress/src/console.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe.rs
- confidence: cited
  path: crates/spindle-ingress/src/router.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/instance_history.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/main.rs
- confidence: cited
  path: crates/spindle-kernel/src/contract.rs
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: inferred
  path: crates/spindle-kernel/src/postgres.rs
- confidence: cited
  path: crates/spindle-kernel/src/sqlite.rs
- confidence: cited
  path: crates/spindle-kernel/src/store.rs
- confidence: cited
  path: crates/spindle-store-browser/src/indexeddb.rs
- confidence: inferred
  path: crates/spindle-telemetry/src/kernel.rs
- confidence: inferred
  path: crates/spindle-wasm/src/worker.rs
revision: 10
---
# A consumer reads the committed events of one entity instance

## Outcome

A consumer reads the committed events of one entity instance (the entity's qualified name, which
names its component, and the identity), oldest first, with position, time, command, caller and
outcome, paged; used to show the revision history of a record that commands change in place.

## Why

Asked for by a downstream consumer on 2026-11-10: a revision history for a record that commands
change in place. Today `GET /api/events` (`crates/spindle-ingress/src/console.rs:90-158`) reads all
committed events from the stores after a position (`story:events-read-from-store`, implemented),
with no per-instance filter, and no store keeps the caller or the outcome beside an event.

## Acceptance

- `an_instance_reads_its_events_oldest_first` (host): after three commands change one instance and
  one command changes another, a read of the first instance answers its three events, oldest first,
  each with its position, time, command, caller and outcome, and none of the other instance's.
- `an_instance_history_pages_with_a_cursor`: a read with a page size of 2 answers two events and a
  cursor; the next page answers the third.
- `an_instance_no_command_changed_reads_empty`: a read of an instance no command changed answers an
  empty list.
- `an_affected_instance_reads_its_group_events`: a command whose outcome `affects:` a second instance
  is in the history of both instances.
- `an_instance_history_survives_a_restart_on_sqlite`: the same read after a restart on a SQLite
  state answers the same events.
- `instance_history_route_answers_its_events` (ingress): `GET /api/history?entity=&identity=` answers
  the read above as JSON; a malformed query answers `400`; the identity is elided in the request
  log.
- `the_store_names_the_instances_each_write_wrote` (store contract): memory and SQLite, and
  PostgreSQL and IndexedDB where their suites run.
- `a_store_written_before_reads_history_only_from_new_writes`: on a SQLite state written by 0.22.0,
  whose writes name no instances, a read of an instance answers only the events of writes made
  after the upgrade, and the changelog says so.

## Open

- Answered when scoped: see `## Decided when scoped` (D1 to D4).

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-host` (the per-instance read on `Host` and `Reader`, filtering `Store::events_after`) — inferred, from where `published_after` lives (`host.rs:1329`, `host/reader.rs:159`)
- **Files:** `crates/spindle-host/src/host.rs` (`published`/`published_after` :1311-1349, `attribute` call :1994, `Published` :158) — cited
- **Files:** `crates/spindle-host/src/host/reader.rs` (`published_after` :159) — cited
- **Files:** `crates/spindle-host/src/host/range.rs` (cursor; a one-store position cursor is added beside `Cursor`) — inferred
- **Files:** `crates/spindle-kernel/src/store.rs` (`Origin` :143, `Committed` :153, `Store` :207, `MemoryStore` `Events::record` :616) — cited
- **Files:** `crates/spindle-kernel/src/sqlite.rs` (`groups_after` :1186, request-id grouping :1207, `origin_body`/`origin_of` :1272-1283) — cited
- **Files:** `crates/spindle-kernel/src/contract.rs` (a store contract case for the instances a write wrote) — cited
- **Files:** `crates/spindle-kernel/src/execute.rs` (outcome named to the store before commit, :1990-2006) — cited
- **Files:** `crates/spindle-store-browser/src/indexeddb.rs` (`Recorded` :47, `Origin` from record :64, `committed` :779) — cited
- **Files:** `crates/spindle-ingress/src/console.rs` (`Console`, `Console::ALL` :30, query parsing `ranged` :170) — cited
- **Files:** `crates/spindle-ingress/src/router.rs` (console dispatch :299-346, :400-421) — cited
- **Files:** `crates/spindle-ingress/src/surface.rs` (console routes from `Console::ALL` :1517, :1626) — cited
- **Files:** `crates/spindle-ingress/src/observe.rs` (request-log query elision keeps only `after`/`limit`, :1070-1100; `entity`/`identity` stay elided) — cited
- **Also likely:** `crates/spindle-telemetry/src/kernel.rs`, `crates/spindle-wasm/src/worker.rs`, `crates/spindle-host/src/host/rows.rs` (wrapper stores pass on a new `Store::outcome`) — inferred, they pass on `attribute`/`events_after` today
- **Also likely:** `crates/spindle-kernel/src/postgres.rs` — inferred, same `LogStore` as SQLite
- **Tests:** `crates/spindle-host/tests/spindle-host/instance_history.rs` + `main.rs` mod line; `crates/spindle-ingress/tests/spindle-ingress/instance_history.rs` + `main.rs` mod line — inferred, one integration binary per crate
- **Documents:** `.engineering/planning/story/console-api.md` may describe console routes — inferred
- **Confidence:** medium — the read and write sites are cited; the route location, the instance semantics, time and outcome storage rest on D1-D4
- **Would collide with:** the ingress console surface (`console.rs`, `router.rs`, `surface.rs`), the store event encoding and `Origin`/`Committed` in `spindle-kernel` and `spindle-store-browser`, the host's command attribution in `host.rs`, and both crates' `tests/*/main.rs` — cited
- **Safety fact:** every implementor builds `Committed` and `Origin` literally (`store.rs:620`, `sqlite.rs:1139`, `indexeddb.rs:64`, test stores in `spindle-host/tests/spindle-host/events_range.rs`), so a new field is caught at compile time; the new `spindle.written` origin key and `Recorded` field read back as absent on old data — step 2, unproven

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: a console route `GET /api/history?entity=&identity=&after=&limit=`, in
  `Console::ALL`, so `story:console-routes-off`'s switch covers it.
- D2 B: the events of an instance are every event of every write that wrote it;
  `Committed` gains the instances its write wrote, and the host filters `events_after`; an
  `affects:` outcome's events are read on each instance of the group.
- D3 A: the time is the command's time from `Origin.sequence`; no stored format
  changes.
- D4 A: a new `Store::outcome(&str)` the kernel calls before it commits, kept beside
  the origin; wrapper stores pass it on.

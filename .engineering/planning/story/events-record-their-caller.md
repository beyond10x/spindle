---
format: aep.planning-md/3
id: story:events-record-their-caller
kind: story
status: active
title: Every committed event records the caller that caused it
summary: Each committed event records the actor, caller attributes and request id of its cause instead of spindle-kernel.
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- informed_by: story:http-caller-proves-actor
scope:
- confidence: cited
  path: crates/spindle-cli/src/seed.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/seed.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/seed/caller.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-host/tests/fixtures
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host
- confidence: cited
  path: crates/spindle-ingress/src/answer.rs
- confidence: cited
  path: crates/spindle-kernel/src/contract.rs
- confidence: cited
  path: crates/spindle-kernel/src/postgres.rs
- confidence: cited
  path: crates/spindle-kernel/src/sqlite.rs
- confidence: inferred
  path: crates/spindle-kernel/src/state_move.rs
- confidence: cited
  path: crates/spindle-kernel/src/store.rs
- confidence: cited
  path: crates/spindle-store-browser/src/indexeddb.rs
- confidence: inferred
  path: docs/design/spindle.md
revision: 11
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T07:28:36Z", actor: "human:operator", revision: 10, decided_on: {"recorded":{"review_outcome":8}}}
- {from: "proposed", to: "active", at: "2026-11-27T07:28:37Z", actor: "human:operator", revision: 11, decided_on: {"recorded":{"review_outcome":8}}}
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
  whose origin names that actor, its attributes and the request id the ingress minted, read back
  through `Store::events_after` on memory, SQLite and PostgreSQL (the store contract).
- `an_event_a_binding_caused_records_its_cause`: an event a binding's command committed names the
  binding and the caller of the event that triggered it.
- `a_split_run_binding_event_records_an_unknown_cause`: in a run split over two processes, an
  event a binding's command committed names the binding and records its cause as not known.
- `a_seed_event_records_the_seed`: an event a seed entry committed names the seed, the entry's actor
  and the entry's `request_id`.
- `a_store_written_before_reads_back_no_caller`: an origin with only `command` and `sequence`, as
  0.22.0 wrote it, reads back with no caller.
- One command's writes keep a fresh event-log `request_id` each, so `events_after` still splits
  them into separate writes.

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: the caller and the request id go into the `origin` body (`spindle.written` and
  the published end); the ingress mints one request id per HTTP request, a seed entry gives its
  `request_id`; the event log's own `request_id` stays fresh for each write.
- D2 A: a binding's command learns its cause inside one process through `staged_by`;
  in a split run the event names the binding and records the cause as not known; no change to
  `spindle_channel::Envelope`.
- D3 A: stores written before this change read back with no caller, without migration.
- D4 B: `GET /api/events` does not answer the caller in this story;
  `story:entity-event-history` answers it per instance.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel` (the attribution a store records and reads back) — cited
- **Files:** `crates/spindle-kernel/src/sqlite.rs:95,2200-2213` (`ACTOR`, `meta`), `:181,688-690` (`LogStore::origin`, `attribute`), `:617-618,710-724` (`origin` written into `spindle.written` / the published end), `:1271-1283` (`origin_body`, `origin_of`) — cited
- **Files:** `crates/spindle-kernel/src/store.rs:139-161` (`Origin`, `Committed`), `:588-640` (memory store `record`) — cited
- **Files:** `crates/spindle-kernel/src/postgres.rs:39-44` (same `LogStore`; no own change expected) — cited
- **Files:** `crates/spindle-host/src/host.rs:1630-1700` (`submit`, `submit_as`, `submit_refusing`), `:1970-2025` (`execute`, `attribute(Some(Origin {…}))` at `:1994`), `:2079-2128` (`invoke`, `attempt`: a binding's command runs with no caller), `:787-790,2057` (`staged_by`) — cited
- **Files:** `crates/spindle-ingress/src/answer.rs:489-495` (HTTP command sent with the proven caller) — cited
- **Files:** `crates/spindle-cli/src/seed.rs:714-721` (`submitted`: seed entry sent with actor, caller; its `request_id` not passed today) — cited
- **Files:** `crates/spindle-kernel/src/contract.rs:1050-1056` (`origin` helper) plus a new contract case — cited
- **Symbols:** `spindle_kernel::Origin`, `Committed::origin`, `Store::attribute`, `sqlite::meta`, `ACTOR`, `origin_body`, `origin_of`, `Host::submit_as`, `Host::submit_refusing`, `Host::execute`, `Host::attempt`, `Cause` (host) — cited
- **Also likely:** `crates/spindle-store-browser/src/indexeddb.rs:44-68` (`Recorded`, `Recorded::origin` builds `Origin` by literal; the contract case runs there too) — cited site, change inferred
- **Also likely:** `crates/spindle-kernel/src/state_move.rs:554-563` (copies actor/request id verbatim; expected unchanged) — inferred
- **Also likely:** new host API carrying the provenance (for example `Host::submit_from`), a new kernel type on `Origin` (name open) — inferred
- **Tests:** `crates/spindle-kernel/src/contract.rs` new case `an_attributed_caller_reads_back` (memory, SQLite, IndexedDB; PostgreSQL via `tests/spindle-kernel/postgres.rs`, ignored without `SPINDLE_TEST_DATABASE_URL`) — inferred
- **Tests:** `crates/spindle-host/tests/spindle-host/` new module with `an_event_records_the_caller_of_its_request`, `an_event_a_binding_caused_records_its_cause` (a fixture with one event binding, existing or new under `tests/fixtures/`) — inferred
- **Tests:** `crates/spindle-cli/tests/spindle-cli/seed/caller.rs` (new module; one `mod` line at the end of `tests/spindle-cli/seed.rs`) `a_seed_event_records_the_seed` — inferred
- **Tests:** unit test in `crates/spindle-kernel/src/sqlite.rs` `a_store_written_before_reads_back_no_caller` (an `origin` with only `command`, `sequence`) — inferred
- **Documents:** `crates/spindle-kernel/src/sqlite.rs:23-27` module doc (the `"origin"` shape); `docs/design/spindle.md` if the stored attribution is named there — inferred
- **Confidence:** medium. The store and host sites are cited. Where the request id lives and how a binding's cause is carried depend on D1 and D2.
- **Would collide with:** units touching `spindle-kernel`'s `LogStore` attribution or `Origin`; `spindle-host`'s submit or binding-invoke path; `spindle-cli/src/seed.rs` (`submitted`); `spindle-ingress/src/answer.rs` command submit; `spindle-store-browser/src/indexeddb.rs` records; `spindle-ingress/src/console.rs` only under D4 B
- **Safety fact:** `LogStore::groups_after` splits the feed into writes by the event log's `request_id` (`sqlite.rs:1205-1214`), so that column must stay fresh for each append group. The caller and the request's id go into the `origin` body. Nothing in spindle reads the event log's `actor` column back, except `state move`, which copies it as is (`state_move.rs:557`), so setting it to the caller's actor is safe. Step 2, unproven.

---
format: aep.planning-md/3
id: story:postgres-store
kind: story
status: draft
title: store.durable bound to postgres keeps state in an operator's PostgreSQL
relations:
- serves: vision:spindle
- decomposes: epic:split-run
scope:
- confidence: cited
  path: Cargo.toml
- confidence: cited
  path: README.md
- confidence: cited
  path: Taskfile.yml
- confidence: cited
  path: crates/spindle-capability/src/environment.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-kernel/Cargo.toml
- confidence: inferred
  path: crates/spindle-kernel/src/postgres.rs
- confidence: cited
  path: crates/spindle-kernel/src/sqlite.rs
- confidence: inferred
  path: examples/environments/postgres.toml
revision: 4
---
# store.durable bound to postgres keeps state in an operator's PostgreSQL

Accepted for the store choice: `approval-record:e4-postgres-store-acceptance` (Timo, 2026-11-10,
"Yeah, exactly. Postgres. That's the deal.").

## Outcome

An environment binding `store.durable` to `postgres` runs each component's store on a PostgreSQL
database the operator deploys, through `eventlog-postgres` at the eventlog tag Spindle already pins
(0.9.2). The connection URL comes from the setting `database_url`
(`SPINDLE_SETTING_DATABASE_URL`), never from the environment document or the specification. Each
component keeps its own table prefix, so components of one deployment can share one database
without reading each other's streams. The store passes the kernel's store contract exactly as the
SQLite store does.

## Acceptance

- `the_postgres_store_passes_the_contract` (ignored without `SPINDLE_TEST_DATABASE_URL`, run by
  `task check:postgres`): every case of `spindle_kernel::contract` passes against PostgreSQL.
- `state_outlives_a_restart_on_postgres`: `run` on rally bound to `postgres`, stopped after a
  command and started again on the same database, answers `MatchState` as before the stop.
- `two_components_share_a_database_apart`: two components on one database each read only their
  own rows.
- `run` with `store.durable` bound to `postgres` and no `database_url` exits 1 before listening,
  naming the setting.

## Scope

- `crates/spindle-kernel/src/postgres.rs` (new, beside `sqlite.rs`, sharing its row and event
  encoding: `ROW`, `REMOVED`, `WRITTEN`, `PREFIX`, `TENANT`, lines 31-48),
  `crates/spindle-kernel/src/sqlite.rs`, `crates/spindle-kernel/Cargo.toml` (a `postgres` feature),
  `Cargo.toml` (`eventlog-postgres` at tag 0.9.2).
- `crates/spindle-capability/src/derive.rs` is not touched: `store.durable` is derived already;
  `postgres` is one more implementation of it in `crates/spindle-capability/src/environment.rs`,
  and a row in the README table.
- `crates/spindle-cli/src/serve.rs` (lines 193-216), `crates/spindle-cli/src/main.rs` (lines
  521-544), `crates/spindle-host/src/host.rs` (store choice, line 664).
- `examples/environments/postgres.toml` (new), `Taskfile.yml` (`check:postgres`).

## Shared files

`story:state-directory-lock` edits `crates/spindle-cli/src/serve.rs` and
`crates/spindle-cli/src/main.rs` too, and runs after this story. `story:events-read-from-store`
edits the `Store` port and `crates/spindle-kernel/src/sqlite.rs`, and runs after this story.

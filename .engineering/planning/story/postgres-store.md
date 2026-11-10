---
format: aep.planning-md/3
id: story:postgres-store
kind: story
status: active
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
- confidence: inferred
  path: crates/spindle-cli/src/cli.rs
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
revision: 9
transitions:
- {from: "draft", to: "proposed", at: "2026-11-10T15:08:56Z", actor: "human:timo", revision: 6}
- {from: "proposed", to: "active", at: "2026-11-10T15:08:57Z", actor: "human:timo", revision: 7}
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

## Hosted database (eventlog 0.9.2, read 2026-11-10)

`eventlog-postgres` at the pinned tag (b10ed05) admits a database that is not on loopback or a
Unix socket only as a hosted one, and the Outcome above must follow it:

- Transport: `PostgresConfig::verified(url, schema, prefix, roots)` sets `sslmode=require` and
  verifies the server against CA roots; plaintext is refused for any non-loopback host
  (`crates/eventlog-postgres/src/pool.rs:169-227`). The store therefore needs a CA bundle setting
  beside `database_url`, and an owner schema name.
- Roles: `PostgresEventStore::migrate` creates the tables with a DDL-capable migration role
  (`lib.rs:130-147`). `PostgresEventStore::open` refuses a role that can run DDL, owns objects in
  the schema, is a member of another role or has superuser, createrole, createdb, replication or
  bypassrls (`lib.rs:192-217`), and requires SELECT, INSERT, UPDATE, DELETE on every
  `<prefix>_*` table, USAGE on the `<prefix>_events` global sequence and TEMP on the database
  (`schema.rs:686-699`). So `run` opens with the application role, and the tables are created by
  a separate migrate step (a subcommand or `--migrate` with the migration role's URL).
- Connections: `open` takes the database's connection budget, the replica count and reserved
  connections, and refuses when `replicas * max_connections + reserved` exceeds it (`lib.rs:193`).
- Server: PostgreSQL 13 or later (eventlog CHANGELOG); CI runs 17.6. No extension; advisory locks
  are used.
- Tables: `<prefix>_events`, `_commands`, `_claims`, `_identity`, `_blobs`,
  `_projection_cursors`, `_snapshots`, `_scope_counters`, `_schema_version`,
  `_projection_registry`, `_snapshot_generations`, `_append_groups` (`schema.rs:27-149`).

Settings this adds to the Outcome: the migration role's URL (migrate only), the CA bundle path, the
owner schema, and the connection budget; the table prefix stays per component and is not a
setting.

## Settings (fixed 2026-11-10, for wave 19)

Settings are read as Spindle reads every setting (`SPINDLE_SETTING_<NAME>`):

- `database_url`: one full URL, `postgres://user:password@host:port/database?sslmode=require`.
  `run` takes the serving role's URL; `store migrate` takes the owner role's URL in the same
  setting, so one name serves the migrate container and the serving container.
- `database_ca`: path of a PEM CA bundle; required for a host that is not loopback or a Unix
  socket (eventlog's `PostgresConfig::verified`). A loopback URL with no CA runs eventlog's
  isolated mode, for tests and a local database.
- `database_schema`: the owner schema, default `public`.
- `database_pool`: connections per component store, default 4.
- `database_connections`: the connection budget the serving role may use; `run` refuses to start
  when components × `database_pool` + 1 exceeds it (eventlog `open`, one reserved connection,
  replicas 1 per process).
- The table prefix is not a setting: `spindle_<component>`, made to pass eventlog's prefix rule.

`b10x-spindle store migrate <spec> --environment <env>` creates every component's tables with
`PostgresEventStore::migrate`, idempotent, exits 0 when they exist, and is the only command that
runs DDL. `run` opens with `PostgresEventStore::open` and so refuses a DDL-capable role, naming the
role's rights eventlog requires.

A downstream deployment checked this against its database on 2026-11-10: Aurora PostgreSQL 17.10,
an owner role and a serving role, serving connection limit 8, one component, one replica, one URL
per role composed by its chart. Pool 4 plus 1 reserved fits.

## Acceptance added

- `store migrate` run twice on an empty database exits 0 both times and leaves the same tables.
- `run` with the owner role's URL exits 1 before listening, naming the DDL right it refuses.
- `run` with `database_connections` below components × pool + 1 exits 1 before listening, naming
  both numbers.
- `run` against a non-loopback host without `database_ca` exits 1 before listening, naming the
  setting.

---
format: aep.planning-md/3
id: story:spec-change-over-stored-rows
kind: story
status: active
title: A specification change over a durable store is refused when its stored rows do not fit
summary: A run on a store an earlier specification wrote starts when its rows stay readable and otherwise exits 3 naming each entity and field.
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- depends_on: story:postgres-lane-kills-slow-tests
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/database.rs
- confidence: inferred
  path: crates/spindle-cli/src/main.rs
- confidence: inferred
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/spec-change-after
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/spec-change-before
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/postgres.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/spec_change.rs
- confidence: inferred
  path: crates/spindle-docs/src/status.rs
- confidence: inferred
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-kernel/src/fit.rs
- confidence: inferred
  path: crates/spindle-kernel/src/lib.rs
- confidence: inferred
  path: crates/spindle-kernel/src/postgres.rs
- confidence: inferred
  path: crates/spindle-kernel/src/sqlite.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/fit.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/main.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/postgres.rs
- confidence: inferred
  path: website/data/status.json
- confidence: inferred
  path: website/docs/concepts/environment-and-admission.md
- confidence: inferred
  path: website/docs/reference/cli.md
- confidence: inferred
  path: website/docs/status.md
revision: 16
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T09:56:32Z", actor: "human:operator", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-27T13:07:33Z", actor: "human:operator", revision: 16, decided_on: {"recorded":{"review_outcome":4}}}
---
# A specification change over a durable store is refused or migrated

## Outcome

A `run` started with a specification other than the one a durable store was last written under
either starts (the change keeps every stored row readable) or exits 3 before listening, naming
each entity and field whose stored rows the new specification cannot read. No migration is
offered (`## Architecture decision`).

A change misfits only for an entity that holds rows: a field removed, a field retyped (types
compared by structure, so a named type redefined under the same name counts), a non-optional field
added, a lifecycle state removed while rows rest in it (named by entity and state), or the entity
removed. This is Spindle's own rule over the recorded shapes (D1 A, D4 A). It holds for a
PostgreSQL store, for the SQLite store a served run's host opens, and for the one SQLite store of a
stdin `run`.

## Why

No store records the specification it was written under: `PostgresStore::open` and
`SqliteStore::open` take no model (`crates/spindle-kernel/src/postgres.rs:173`,
`crates/spindle-kernel/src/sqlite.rs:239`), and `run` opens each component's PostgreSQL store from
the `database_*` settings alone (`crates/spindle-cli/src/database.rs:130-136`). A consumer's
service on a PostgreSQL store that ships a new specification version reads its old rows under the
new types, with nothing to say a field was removed or retyped.

## Acceptance

- `a_compatible_change_starts_on_the_old_store`: adding an optional field to an entity, then
  starting on a store the old specification wrote, starts and answers the old rows.
- `an_incompatible_change_exits_before_listening`: removing or retyping a field that stored rows
  hold exits 3 naming the entity and the field on stdout, prints no listening line, and leaves the
  rows and the record unchanged.
- `the_store_records_the_specification_it_was_written_under`: after a write, the store holds the
  model's digest, read back on the next open.
- The misfit rule (D1 A, D4 A), in `crates/spindle-kernel/tests/spindle-kernel/fit.rs`:
  `an_optional_field_added_fits_stored_rows`, `a_removed_field_misfits_only_where_rows_hold_it`,
  `a_retyped_field_misfits_through_a_named_type`, `a_required_field_added_misfits_stored_rows`,
  `a_removed_entity_holding_rows_misfits`, `a_removed_state_holding_rows_misfits`,
  `an_empty_store_fits_any_specification`.
- `a_store_without_a_record_starts_and_records`: a store with rows and no record starts, records
  the current model and prints one line saying it had no record (D2 A).
- `the_record_is_not_a_published_event`: the record shows neither in `GET /api/events` nor in
  `events_after`.
- `a_stdin_run_refuses_a_misfit_state_file`: a stdin `run` with `spec-change-after` on a SQLite
  state file `spec-change-before` wrote exits 3 naming the entity and the field on stdout, as
  `an_incompatible_change_exits_before_listening` does, and leaves the rows and the record
  unchanged; an exit 3 from admission alone does not pass it.
- `the_postgres_store_records_the_specification_it_was_written_under` and
  `an_incompatible_change_exits_before_listening_on_postgres`: the same on PostgreSQL, in the
  `task check:postgres` lane.

The process tests are in `crates/spindle-cli/tests/spindle-cli/spec_change.rs`, on the fixtures
`spec-change-before` and `spec-change-after`.

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible.

- D1 A: a misfit is Spindle's own rule over the recorded shapes, counted only for entities that
  hold rows (field removed, field retyped by structure, non-optional field added, entity removed),
  and an ESS gap asks `ess-diff` to classify entity field changes for history so the rule can move
  there; answered A by the host operator on 2026-11-27. Not taken: B, `ess-diff`
  0.99.2's history dimension, which answers `unknown` for every entity field change and fails
  acceptance 1 or 2; C, B after an ESS release that classifies them.
- D2 A: a store with rows and no record starts, records the current model and prints one line
  saying it had no record.
- D3 A: the record is written at open, after the check passes, and only when the stored digest
  differs from the current one.
- D4 A: a lifecycle state removed while rows rest in it is a misfit, named by entity and state.


The ESS gap is filed in the ESS repository as a section of
`story:verify-diff-decides-what-the-model-settles` (draft, test `entity_stored_history` planned);
this rule moves to `ess-diff` once a release classifies entity field changes for history.

## Architecture decision

Option A, picked by the operator on 2026-11-27: a store records the model's digest and the shapes
of its entities; `run` refuses to start on a store whose shapes do not fit the specification and
exits 3, naming each incompatible entity and field. No migration.

Not taken: B (A plus a migration mapping in the environment document), C (A plus a `state migrate`
verb), D (defer).

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel` (`LogStore`, which serves both SQLite and PostgreSQL) — cited, `sqlite.rs:31-33` says PostgreSQL is "the same store"
- **Files:** `crates/spindle-kernel/src/sqlite.rs`: the record kept in a stream of type `spindle:log` beside `MARKS` (`:104-108`), read and appended through `LogStore`, and skipped where the feed is read back (`events_after`, `:1351`) — inferred
- **Files:** `crates/spindle-kernel/src/fit.rs` (new): entity shapes from `EssIr`, the model digest (`SuiteProvenance::of(&ir).spec_digest`, as `spindle-conformance/src/lib.rs:125` reads it), the comparison and a `Misfit` naming entity and field — inferred
- **Files:** `crates/spindle-kernel/src/lib.rs`: `mod fit` and a re-export — inferred
- **Files:** `crates/spindle-cli/src/database.rs:129-134` (`stores`): check each `PostgresStore` before it is boxed — cited
- **Files:** `crates/spindle-host/src/host.rs`: the SQLite open in `Host::assemble` (`:1121-1127`) checked against `owned(&ir, name)` (`:3015`), and a new `HostError` variant (`:257`) — inferred
- **Files:** `crates/spindle-cli/src/serve.rs`: map the PostgreSQL misfit (`:337`) and the host's (`:468-474`) to exit 3 before listening — inferred
- **Files:** `crates/spindle-cli/src/main.rs:859-871`: the stdin `run` SQLite store, checked against every entity — inferred
- **Symbols:** `PostgresStore::open`, `SqliteStore::open`, `database::stores` — cited; `LogStore`, `MARKS`, `Fold::of`, `HostError`, `owned` — inferred
- **Also likely:** `crates/spindle-kernel/src/postgres.rs` (a doc comment of `open` at most; the record code is generic over `LogStore`) — inferred
- **Also likely:** `crates/spindle-kernel/src/state_move.rs`, no code change: `read` (`:130`) and `unnamed` (`:184-186`) already treat stream type `spindle:log` as no entity, and the move copies every event, so the record moves with the store — inferred
- **Documents:** `crates/spindle-cli/src/cli.rs` with the generated `website/docs/reference/cli.md` (exit 3 of `run`), `website/docs/concepts/environment-and-admission.md` ("State in PostgreSQL", `:372`), `CHANGELOG.md` — inferred
- **Documents:** `crates/spindle-docs/src/status.rs` (a row for the new capability; `AGENTS.md:149` asks a capability change to change the site) with the generated `website/data/status.json` and `website/docs/status.md` — inferred (added 2026-11-27 on the plan critics' finding)
- **Tests, default lane** (no PostgreSQL, no container, no network beyond a loopback listener; each bound 60 s, the step budget 120 s): in `crates/spindle-kernel/tests/spindle-kernel/fit.rs`: `the_store_records_the_specification_it_was_written_under` (acceptance 3, SQLite), `an_optional_field_added_fits_stored_rows`, `a_removed_field_misfits_only_where_rows_hold_it`, `a_retyped_field_misfits_through_a_named_type`, `a_required_field_added_misfits_stored_rows`, `a_removed_entity_holding_rows_misfits`, `an_empty_store_fits_any_specification`, `a_store_without_a_record_starts_and_records`, `the_record_is_not_a_published_event`. In `crates/spindle-cli/tests/spindle-cli/spec_change.rs`, with two fixture specifications `crates/spindle-cli/tests/fixtures/spec-change-before` and `spec-change-after`: `a_compatible_change_starts_on_the_old_store` (acceptance 1), `an_incompatible_change_exits_before_listening` (acceptance 2: exit 3, entity and field on stdout, no listening line, rows and record unchanged), `a_stdin_run_refuses_a_misfit_state_file` — inferred
- **Tests, PostgreSQL lane** (`task check:postgres`, needs `SPINDLE_TEST_DATABASE_URL`; `#[ignore]` in the default lane): `the_postgres_store_records_the_specification_it_was_written_under` in `crates/spindle-kernel/tests/spindle-kernel/postgres.rs`, `an_incompatible_change_exits_before_listening_on_postgres` in `crates/spindle-cli/tests/spindle-cli/postgres.rs` — inferred
- **Confidence:** medium. The store, open and refusal sites are read from the tree; the new module, the host hunk and the test names are this scoper's design
- **Would collide with:** `LogStore` in `spindle-kernel/src/sqlite.rs` and its feed read-back; `database::stores` and the store opening in `serve.rs`; the store opening and `HostError` in `spindle-host/src/host.rs`; `run`'s docs in `cli.rs` and `cli.md`; the "State in PostgreSQL" section of the environment docs
- **Shared with wave 30:** `spindle-host/src/host.rs` (U1, U6, U7, U12; a separate hunk in `Host::assemble` and `HostError`); `spindle-kernel/src/lib.rs` (U5, U6); `spindle-cli/src/main.rs` (U9); `spindle-cli/src/cli.rs` with `cli.md` (U9, U12); the test `main.rs` module lists of `spindle-kernel` (U3, U6, U11) and `spindle-cli` (U9); `CHANGELOG.md` (every unit) — cited, from `wave-30.md` § Surfaces
- **Shared with wave 31:** `story:event-egress-and-dead-letters` (an outbox cursor and a dead-letter table in the SQLite and PostgreSQL stores, so `sqlite.rs` `LogStore` and the feed filter; `serve.rs`; the "State in PostgreSQL" docs); `story:several-writers-one-postgres` (`database.rs`, `serve.rs`, `postgres.rs`, the docs section); `story:listener-tls` and `story:verified-caller-token` (`serve.rs`, `cli.rs`, `cli.md`); `story:spindle-image-published` none — inferred from their bodies
- **depends_on:** `story:postgres-lane-kills-slow-tests`, added 2026-11-27 on the plan critics' finding: the PostgreSQL acceptance lines rely on that lane's 60 s kill — cited (the plan). Proposed edge, recorded: `story:several-writers-one-postgres` depends_on this one, because a second replica started on a newer specification is the rolling-deploy case this check guards — inferred
- **Safety fact:** the record lives as events in the store's existing eventlog tables under stream type `spindle:log`. So it needs no DDL (a hosted role "must run no DDL", `postgres.rs:166`), the SQLite open fold ignores it (`Fold::of`, `sqlite.rs:1909-1918`), and `state move` carries it (`state_move.rs:130`, `:184-186`). Feed read-back skips only the `MARKS` stream by name (`sqlite.rs:1351`), so the record surfaces in `GET /api/events` unless that line also skips it. Proof level 2, unproven

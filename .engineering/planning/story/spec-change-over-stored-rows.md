---
format: aep.planning-md/3
id: story:spec-change-over-stored-rows
kind: story
status: proposed
title: A specification change over a durable store is refused or migrated
summary: A run on a store an earlier specification wrote starts when its rows stay readable and otherwise exits 3 naming each entity and field.
relations:
- serves: vision:spindle
- decomposes: epic:production-service
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T09:56:32Z", actor: "human:operator", revision: 3}
---
# A specification change over a durable store is refused or migrated

## Outcome

A `run` started with a specification other than the one a durable store was last written under
either starts (the change keeps every stored row readable) or exits 3 before listening, naming
each entity and field whose stored rows the new specification cannot read, with a way to migrate
them.

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
  hold exits 3 naming the entity and the field, and writes nothing.
- `the_store_records_the_specification_it_was_written_under`: after a write, the store holds the
  model's digest, read back on the next open.

## Decided when scoped

- What counts as compatible (as ESS classifies a specification change, not a second rule here),
  and the shape of a migration: a command, a mapping in the environment, or refusal only.

## Architecture decision

Option A, picked by the operator on 2026-11-27: a store records the model's digest and the shapes
of its entities; `run` refuses to start on a store whose shapes do not fit the specification and
exits 3, naming each incompatible entity and field. No migration.

Not taken: B (A plus a migration mapping in the environment document), C (A plus a `state migrate`
verb), D (defer).

---
format: aep.planning-md/3
id: story:seed-record-after-a-component-is-removed
kind: story
status: draft
title: A seed record survives the removal of the component whose lease table held it
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: AGENTS.md
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/database.rs
- confidence: inferred
  path: crates/spindle-cli/src/seed.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/postgres/replicas.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/postgres/replicas/adversary_w32_u4.rs
- confidence: cited
  path: crates/spindle-kernel/src/postgres.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/postgres.rs
- confidence: inferred
  path: docs/design/spindle.md
- confidence: cited
  path: website/docs/concepts/environment-and-admission.md
- confidence: cited
  path: website/docs/reference/cli.md
revision: 8
---
## Outcome

A leased run on PostgreSQL finds its seed record also when a later release removed the component
whose lease table held it, so another seed file is still refused and a part-done seed still resumes.

When no current component's lease table holds a record, the start looks once, with one `pg_tables`
query, for lease tables of no current component: found by table name (`spindle_%_p_lease`), and
read only where the table holds a `lease` row (D2 A). A record found there is moved to the first
component's table in one statement under two leases: that table's own, which the mover takes with
`TAKE` and releases after, and the first component's. While another process, such as one of the
old release, holds the orphan table's lease, the record is read and not moved (D1 A). Several
records follow wave 32's rule: alike they are one, differing they exit 1 naming the tables; one
schema holds one leased deployment (D3 A).

## Why

Wave 32's U4 (`story:seed-under-a-lease`) keeps the record in the `seed` row of the first
component's `<prefix>_p_lease` table. Since its U4 adversary rounds 2 and 3, a start reads the
record from every current component's lease table (`Leases::read`,
`crates/spindle-cli/src/database.rs:505-539`), records that differ exit 1 naming the tables
(`:520-531`), and the next write gathers them into the first component's table
(`Lease::gather_seed`, `crates/spindle-kernel/src/postgres.rs:795`, its statement `seed_gathering`
`:918`). A record kept only in the table of a component the current specification no longer has is
not read: the store holds rows, no record is found, and D5 B takes the seed as applied (a complete
record is written over the loss).

Reaching the old table needs a scan of the schema's lease tables by table name: a long or unusual
component name gets a hashed prefix (`spindle_kernel::prefix`, `postgres.rs:101-135`), which the
removed component's name is no longer there to derive, and `Lease::table` (`:533`) runs
`PostgresStore::migrate` for a component on an isolated database, which a removed component cannot
go through. No current run takes the removed component's lease, so the move out of its table is
unguarded unless the mover takes that lease itself.

`crates/spindle-cli/src/cli.rs:569`, `website/docs/reference/cli.md:153` and
`website/docs/concepts/environment-and-admission.md:323` already link this story.

## Acceptance

Each test below is a PostgreSQL-lane test (`#[ignore]`, run by `task check:postgres`, matched by
its `postgres::` filter), each under the 60 s bound and expected under 10 s; the unit's report
names each one's time. No new fixture.

- `a_seed_left_part_done_resumes_after_the_component_holding_its_record_was_removed`
  (`crates/spindle-cli/tests/spindle-cli/postgres/replicas/adversary_w32_u4.rs`, beside
  `grown_seeded`): release 1 is `crates/spindle-cli/tests/fixtures/adversary-w32-u4-grown`
  (components `audit`, `board`, `ledger`) and leaves the record in `audit`'s lease table at `done`
  1 of 3; release 2 is `crates/spindle-cli/tests/fixtures/ingress` (`board`, `ledger`), started
  with the same file. It sends entries 2 and 3 only (its end line counts 2 sent), and afterwards
  `board`'s lease table holds the record at `done` 3 and `audit`'s holds none.
- `another_seed_file_after_the_component_holding_the_record_was_removed_is_refused` (same file):
  release 2 with another seed file exits 1 before listening, naming both digests, prints no
  listening line, and `audit`'s record is unchanged.
- `an_orphan_lease_table_is_found_by_its_table_name`
  (`crates/spindle-kernel/tests/spindle-kernel/postgres.rs`): the scan finds the lease table of a
  component the current specification does not name, including one whose name is over 31 bytes
  (a hashed prefix). The schema is shared with other tests: the test asserts inclusion, never set
  equality.
- `a_table_matching_the_pattern_without_a_lease_row_is_not_read` (kernel `postgres.rs`): a table
  whose name matches `spindle_%_p_lease` and that holds a `seed` row and no `lease` row is not
  read. In a `LIKE` pattern `_` matches any one character, so the name alone admits more than lease
  tables.
- `a_deposed_process_cannot_move_the_seed_record_out_of_a_removed_components_table` (kernel
  `postgres.rs`, modelled on `a_seed_record_moves_between_lease_tables_only_under_both_leases`
  `:937`; D1 A): handle A holds the first component's lease and the orphan table's; handle B takes
  the first component's lease at the next token; A's move is refused by name and both tables are
  unchanged.
- `an_orphan_record_under_another_holders_lease_is_read_not_moved` (kernel `postgres.rs`, D1 A):
  while another handle holds the orphan table's lease, as an old-release process would, the take
  of that lease fails, the record still reads, and neither table changes.
- The `--seed` help (`crates/spindle-cli/src/cli.rs:555-575`, generated `cli.md:153`) and § A seed
  on PostgreSQL with a lease of `environment-and-admission.md` (`:307-336`) state the
  removed-component case in place of the link to this story (`cli.rs:569`, `:323`), and the page
  says one schema holds one leased deployment (D3 A); `docs/design/spindle.md:437`, the module doc
  of `crates/spindle-cli/src/seed.rs` (`:33-42`) and `AGENTS.md:40` change where they say where
  the record is read; the change goes under `[Unreleased]` in `CHANGELOG.md`.

## Decided when scoped

Taken 2026-11-28 on the operator's pick (D-W33b A): the scoper's default, option A, of every
decision, each reversible. No new dependency.

- D1 Guard of the move out of an orphan table: A, the mover takes that table's own lease with
  `TAKE`, gathers under it and the first component's lease in one statement, then releases it;
  while an old-release process holds it, the record is read, not moved.
  Not taken: B (the first component's lease alone; an old-release process could then write the
  record there again).
- D2 When to scan: A, only when no current component's table holds a record, with one `pg_tables`
  query on that start.
  Not taken: B (on every read).
- D3 Several orphan records, or one schema shared by two deployments: A, wave 32's rule (alike are
  one record, differing exit 1 naming the tables); the docs say one schema holds one leased
  deployment.
  Not taken: B (a component-set field in the record).

## Scope

Derived 2026-11-28 by `story-scoper` on `plan/wave-33` (276298bb). Every line is **cited** (read from the story or the tree) or **inferred** (a reading that could be wrong).

- **Stale in the story:** (1) since wave 32 round 3, `Leases::answer` gathers the record from every table found (`Lease::gather_seed`), differing records exit 1 naming the tables (`database.rs:516-531`); the orphan case goes through the same rules. (2) The table pattern is `spindle_%_p_lease`; long or unusual names get a hashed prefix (`spindle_kernel::prefix`, `postgres.rs:101-135`), so an orphan table is handled by its table name; `Lease::table()` runs `PostgresStore::migrate` for a component, which a removed component cannot go through. (3) Three doc places already link this story: `crates/spindle-cli/src/cli.rs:567`, `website/docs/reference/cli.md:153`, `website/docs/concepts/environment-and-admission.md:323`. (4) No new fixture: release 1 `tests/fixtures/adversary-w32-u4-grown` (`audit`, `board`, `ledger`), release 2 `tests/fixtures/ingress` (`board`, `ledger`); `audit` is the dropped `a`.
- **Primary surface:** `crates/spindle-kernel` (`Lease` seed record) and `crates/spindle-cli` (`database::Leases`) — cited
- **Files:** `crates/spindle-kernel/src/postgres.rs:690-965` (`Lease::seed_record`, `gather_seed`, `seed_statement`, `table`, `lease_table`, `seed_gathering`, `TAKE` at 994) — cited
- **Files:** `crates/spindle-cli/src/database.rs:432-546` (`Leases::answer`, `Leases::read` 505-539 reads only `held`, `Leases::elsewhere`, `Leases::table_of`) — cited
- **Symbols:** a schema scan for lease tables of no current component by table name, table-name forms of read and gather (e.g. `Lease::seed_tables`, `Lease::seed_record_in`), take and release of an orphan table's lease row by table name — inferred
- **Tests, lane** (PostgreSQL, `task check:postgres`): in `crates/spindle-cli/tests/spindle-cli/postgres/replicas.rs` or `replicas/adversary_w32_u4.rs` (has `grown_seeded`, `GROWN`): `a_seed_left_part_done_resumes_after_the_component_holding_its_record_was_removed` (record in `audit`'s table `done` 1 of 3; release 2 sends 2; afterwards `board`'s table holds it, `audit`'s none); `another_seed_file_after_the_component_holding_the_record_was_removed_is_refused` (exit 1, both digests, no listening line). Expected under 10 s each, not timed.
- **Tests, kernel** (PostgreSQL): `crates/spindle-kernel/tests/spindle-kernel/postgres.rs` (model: `a_seed_record_moves_between_lease_tables_only_under_both_leases` at 937): `a_deposed_process_cannot_move_the_seed_record_out_of_a_removed_components_table`; `an_orphan_lease_table_is_found_by_its_table_name` incl. a hashed prefix (name over 31 bytes). Shared schema: assert inclusion, never set equality.
- **Also likely:** `crates/spindle-cli/src/seed.rs:30-45` doc of D5 B; no code change expected — inferred
- **Documents:** `crates/spindle-cli/src/cli.rs:555-575`, `website/docs/reference/cli.md:153`, `website/docs/concepts/environment-and-admission.md:307-336` — cited; `docs/design/spindle.md:437`, `CHANGELOG.md`, `AGENTS.md:40` — inferred
- **Would collide with:** units on `spindle-kernel/src/postgres.rs` around `Lease` (fence, `TAKE`/`RENEW`, statement timeouts), the `run --seed` help and docs, `CHANGELOG.md`, `AGENTS.md`
- **Safety fact:** the gather writes nothing unless every lease row it names carries the mover's token and `pg_backend_pid()`, all locked in one statement (`seed_gathering`, `postgres.rs:913-960`) — step 2, unproven
- **Checked when planned (2026-11-28, read against 276298bb):** the fixtures are
  `crates/spindle-cli/tests/fixtures/adversary-w32-u4-grown` and
  `crates/spindle-cli/tests/fixtures/ingress`; the link in the `--seed` help is at `cli.rs:569`, not
  `:567`; the check that records differ is in `Leases::read` (`database.rs:520-531`); `Lease::table`
  migrates only on an isolated database (no CA, `postgres.rs:534`) — cited

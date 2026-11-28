---
format: aep.planning-md/3
id: story:entity-invariant-over-stored-rows
kind: story
status: draft
title: A specification change that tightens an entity invariant is judged against the stored rows
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/spec-change-invariant
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/postgres.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/spec_change.rs
- confidence: inferred
  path: crates/spindle-docs/src/status.rs
- confidence: inferred
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/fit.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/fit.rs
- confidence: inferred
  path: website/data/status.json
- confidence: cited
  path: website/docs/concepts/environment-and-admission.md
- confidence: cited
  path: website/docs/reference/cli.md
- confidence: inferred
  path: website/docs/status.md
revision: 11
---
## Outcome

A specification change is judged against the stored rows also under each entity's own invariants:
whenever the recorded digest differs, every invariant the new model declares on an entity is
evaluated over every stored row of it, as a row at rest is judged (`rests`,
`crates/spindle-kernel/src/execute.rs:60-79`). `run` exits 3 before listening, printing one line per
broken invariant that names the entity, the invariant's statement and the number of stored rows
that break it (D2 A), and starts when every stored row holds every invariant. A row on which an
invariant evaluates `Unknown` fits. The record format is unchanged, so a row that already broke an
unchanged invariant now refuses the next start under another digest (D1 A). On SQLite and on
PostgreSQL alike.

## Why

Found during wave 31's U5 correction round: the spec-change check reads each stored value under a
changed newtype or struct (prefix, alphabet, invariants) but not under an entity's own invariants,
so a run starts over rows its new specification does not admit. At 276298bb `Shape` holds only
`fields` and `states` (`crates/spindle-kernel/src/fit.rs:79-85`), `Shapes::misfits`
(`fit.rs:142-247`) compares only those, and
`website/docs/concepts/environment-and-admission.md:692` says an invariant added to the entity
itself is not checked against the rows.

## Acceptance

Each test is expected under 5 s; the unit's report names each one's time and the time of the
existing `spec_change.rs` cases.

- `an_entity_invariant_a_stored_row_breaks_misfits`
  (`crates/spindle-kernel/tests/spindle-kernel/fit.rs`, default test step, SQLite in a temporary
  directory): with an invariant on the entity that one stored row breaks, `Shapes::misfits`
  returns one invariant misfit naming the entity, the statement and 1 row.
- `an_entity_invariant_every_stored_row_holds_fits` (same file): no misfit.
- `an_entity_invariant_a_stored_row_leaves_unknown_fits` (same file): a row on which the invariant
  evaluates `Unknown` fits.
- `an_entity_invariant_a_stored_row_breaks_exits_before_listening`
  (`crates/spindle-cli/tests/spindle-cli/spec_change.rs`, default test step): a book stored under
  `spec-change-before` breaks the invariant of the new fixture
  `crates/spindle-cli/tests/fixtures/spec-change-invariant` (`spec-change-before` plus an invariant
  on `archive.shelf.Book`, such as `title.count <= 10`); `run` exits 3 before listening, its stdout
  names `archive.shelf.Book`, the statement and 1 row, and the recorded specification and the row
  are unchanged.
- `an_entity_invariant_every_stored_row_holds_starts` (same file, same fixture): every stored book
  holds the invariant, and the run listens.
- `an_entity_invariant_a_stored_row_breaks_refuses_a_stdin_run` (same file, default test step,
  SQLite, through the file's `stdin_run` helper as `a_stdin_run_refuses_a_misfit_state_file`
  `:281` uses it): a stdin `run --state` with `spec-change-before` shelves a book that breaks the
  invariant (`archive.shelf.ShelveBook`); a stdin `run --state` on the same file with
  `spec-change-invariant` exits 3 before reading stdin, its stdout names `archive.shelf.Book`, the
  statement and 1 row, and the recorded specification and the row are unchanged. The test waits
  only on the child's exit; it ends well within the 60 s bound.
- `an_entity_invariant_a_stored_row_breaks_exits_before_listening_on_postgres`
  (`crates/spindle-cli/tests/spindle-cli/postgres.rs`, `#[ignore]`, `task check:postgres`): the
  first `spec_change.rs` case on PostgreSQL, with the same outcome.
- § A specification change over stored rows of `environment-and-admission.md` (`:676-692`) lists a
  broken entity invariant among the misfits and drops the sentence at `:692`; `run`'s description
  (`crates/spindle-cli/src/cli.rs:298`) and the generated `website/docs/reference/cli.md:123` name
  it; the row in `crates/spindle-docs/src/status.rs:145` and the generated
  `website/docs/status.md:28` and `website/data/status.json` name it; the fix goes under
  `[Unreleased]` `### Fixed` in `CHANGELOG.md`.

## Decided when scoped

Taken 2026-11-28 on the operator's pick (D-W33b A): the scoper's default, option A, of every
decision, each reversible. No new dependency, no `depends_on` edge.

- D1 Which invariants: A, every invariant the new model declares, over every stored row, whenever
  the digest differs; the record format is unchanged, so a row that already broke an unchanged
  invariant now refuses the next start.
  Not taken: B (only the invariants missing from the recorded shape, which changes the record
  format).
- D2 The report: A, one line per broken invariant, naming the statement and the row count.
  Not taken: B (one line per entity).

## Scope

Derived 2026-11-28 by `story-scoper` on `plan/wave-33` (276298bb). Every line is **cited** (read from the story or the tree) or **inferred** (a reading that could be wrong).

- **Still true:** `Shape` holds only `fields` and `states` (`crates/spindle-kernel/src/fit.rs:76-83`); `Shapes::misfits` (`fit.rs:139-245`) compares only those; `website/docs/concepts/environment-and-admission.md:692` says an invariant added to the entity is not checked.
- **Primary surface:** `crates/spindle-kernel` (the misfit rule over recorded shapes) — cited
- **Files:** `crates/spindle-kernel/src/fit.rs` (`Shapes::misfits` 139-245, `Held::of` 438-466, `Misfit` and `Display` 279-406) — cited
- **Symbols:** `Shapes::misfits`, `Held`, `Misfit` (new variant such as `Misfit::Invariant { entity, invariant, rows }`, with `Display`, `entity()`, `field()` arms) — cited
- **Symbols:** `Facts::row` (`crates/spindle-kernel/src/facts.rs:112`) and `Predicate::evaluate`, as `rests` uses them (`crates/spindle-kernel/src/execute.rs:60-79`): each stored row judged as `at_rest` judges it, `Unknown` concluding nothing — cited
- **Also likely:** `crates/spindle-kernel/src/execute.rs` only if `rests` becomes pub(crate) — inferred
- **Tests:** `crates/spindle-kernel/tests/spindle-kernel/fit.rs`: `an_entity_invariant_a_stored_row_breaks_misfits`, `an_entity_invariant_every_stored_row_holds_fits`, `an_entity_invariant_a_stored_row_leaves_unknown_fits` (SQLite temp dir) — inferred
- **Tests:** `crates/spindle-cli/tests/spindle-cli/spec_change.rs`: `an_entity_invariant_a_stored_row_breaks_exits_before_listening` (exit 3, stdout names `archive.shelf.Book` and 1 row, record and row unchanged), `an_entity_invariant_every_stored_row_holds_starts`, `an_entity_invariant_a_stored_row_breaks_refuses_a_stdin_run` (the stdin `run --state` path, through `stdin_run` `:255`, as `a_stdin_run_refuses_a_misfit_state_file` `:281`; the stdin store is opened and fitted in `crates/spindle-cli/src/main.rs:983-990`, its own call apart from the served path) — inferred
- **Tests:** `crates/spindle-cli/tests/spindle-cli/postgres.rs`: `an_entity_invariant_a_stored_row_breaks_exits_before_listening_on_postgres` (`#[ignore]`, `task check:postgres`) — inferred
- **Fixture:** `crates/spindle-cli/tests/fixtures/spec-change-invariant/` (`system.yaml`, `domains/shelf.yaml`): `spec-change-before` plus an invariant on `archive.shelf.Book`, e.g. `title.count <= 10` — inferred
- **Documents:** `website/docs/concepts/environment-and-admission.md:676-692` (add the misfit, drop the sentence at 692), `website/docs/reference/cli.md:123` (misfit kinds) — cited
- **Documents:** `crates/spindle-docs/src/status.rs:145`, regenerated `website/docs/status.md:28` and `website/data/status.json`, `CHANGELOG.md` `### Fixed` — inferred; U3 of wave 33 may edit the "Seed at start" row of the same file (`:488`)
- **Would collide with:** `fit.rs`/`Misfit`, the spec-change section of `environment-and-admission.md`, the run section of `cli.md`, status rows, `CHANGELOG.md`
- **Safety fact:** every durable open under another specification reaches `Shapes::misfits` through `LogStore::check` (`crates/spindle-kernel/src/sqlite.rs:2016-2020`); callers `host.rs:3077`, `host.rs:3129`, `spindle-cli/src/main.rs:987`, `spindle-cli/src/database.rs:194`; outside `fit.rs` `Misfit` is only displayed (`database.rs:52`, `HostError::Misfit`) — level 2, unproven
- **Checked when planned (2026-11-28, read against 276298bb):** the claims hold; the lines moved:
  `Shape` `fit.rs:79-85`, `Shapes::misfits` `:142-247`, `Misfit` `:268-350`, its `Display`
  `:352-404`, `Held` `:443-481`. Added: `crates/spindle-cli/src/cli.rs` (`:298`, `run`'s
  description, from which `website/docs/reference/cli.md:123` is generated) — cited
- **Revised after plan-critic round 1 (2026-11-28, read against fc19c7d9):** the stdin `run --state`
  test added (default taken over a `## Not covered` line) — cited

---
format: aep.planning-md/3
id: story:state-moves-to-postgres
kind: story
status: implemented
title: A state directory's streams move into a PostgreSQL store
relations:
- serves: vision:spindle
- decomposes: epic:split-run
- depends_on: story:postgres-store
scope:
- confidence: inferred
  path: README.md
- confidence: inferred
  path: Taskfile.yml
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: inferred
  path: crates/spindle-cli/src/state.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli
- confidence: inferred
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-kernel/Cargo.toml
- confidence: inferred
  path: crates/spindle-kernel/src/lib.rs
- confidence: inferred
  path: crates/spindle-kernel/src/postgres.rs
- confidence: cited
  path: crates/spindle-kernel/src/sqlite.rs
- confidence: inferred
  path: crates/spindle-kernel/src/state_move.rs
- confidence: cited
  path: website/docs/reference/cli.md
- confidence: inferred
  path: website/docs/status.md
revision: 10
transitions:
- {from: "draft", to: "proposed", at: "2026-11-10T15:08:57Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-10T15:08:57Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-11T20:31:02Z", actor: "human:timo", revision: 10, decided_on: {"recorded":{"test_result":1,"review_outcome":2}}}
---
# A state directory's streams move into a PostgreSQL store

## Outcome

An operator moves the state an existing `--state` directory holds (one `<component>.sqlite` file
per component, `crates/spindle-host/src/host.rs:728`) into the PostgreSQL store of
`story:postgres-store`, with Spindle stopped, and the run started on PostgreSQL answers every view
as the run on the directory did.

The move is a subcommand, `b10x-spindle state move --from <dir> <spec> --environment <env>`, with
the PostgreSQL settings of `story:postgres-store` (`database_url` names a role that may write the
migrated tables). It reads each component's feed from SQLite (`SqliteEventStore::open_existing`,
which creates nothing) and appends it, group by group, into the component's prefix with `Expected`
versions, so every stream keeps its versions. Event ids, `recorded_at` and `global_seq` get new
values; Spindle reads none of them (`crates/spindle-kernel/src/sqlite.rs:156-172,276-321`).

## Why

Asked for by a downstream consumer on 2026-11-10 that runs Spindle with `--state` on a volume and
wants its state in a managed PostgreSQL before real data is entered. Both stores are eventlog
stores holding the same streams (`crates/spindle-kernel/src/sqlite.rs:1-13`), so the move is a copy
of streams, versions kept, not a translation. eventlog 0.9.2 has no copy into PostgreSQL that keeps
event ids (`eventlog-postgres` mints an id per append; `SqliteEventStore::restore_events` and
`eventlog_tree::copy` write only into SQLite).

## Acceptance

- `state_moves_into_postgres_and_answers_as_before` (ignored without
  `SPINDLE_TEST_DATABASE_URL`, run by `task check:postgres`): after commands on rally with
  `--state <dir>`, `store migrate` and `state move` into an empty database exit 0, and `run` bound to
  `postgres` on that database answers `MatchState` and every other rally view as the run on `<dir>`
  did.
- `a_moved_instance_continues_at_its_version`: a command decided after the move names the version
  the directory's last write left and commits above it (its events plus `spindle.row`), and a
  commit naming an older version is refused as a conflict (review-result:w19-u4-adversary-1,
  note 3).
- `a_move_into_a_non_empty_prefix_writes_nothing`: when any component's prefix holds the stream of
  an entity the specification declares or the directory holds, or a partial move's mark, the move
  exits 1 before writing anything, naming the component, also while another write transaction is
  open on the server; the check only reads.
- A second move into the same schema while one runs waits up to 5 s, then exits 1 naming the other
  move, writing nothing (review-result:w19-u4-adversary-2).
- A directory written by 0.7.0 to 0.10.0 arrives in the current event format and is scanned by
  stream; one with writes from before 0.7.0 arrives marked, and loses no row.
- A row or event holding a string PostgreSQL refuses (U+0000) exits 1 before writing to any prefix,
  naming the component and the stream (review-result:w19-u4-adversary-1, findings 1 and 2).
- A directory with no `<component>.sqlite` of the specification's components exits 1, naming the
  directory.

## Not covered

- A move while another process uses the directory is not refused; `story:state-directory-lock`
  (draft) would refuse it. The operator stops Spindle first.
- A move that fails partway leaves a partial copy, which the next move refuses. The operator drops
  the component's tables (or the schema), runs `store migrate`, and moves again.
- The stdin form of `run --state` keeps one file with no component name; it is not taken.
- A directory whose event history holds U+0000 is refused and stays unmovable: events are not
  changed, so no edit of current state clears it.

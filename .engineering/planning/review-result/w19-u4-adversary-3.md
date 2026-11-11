---
format: aep.planning-md/3
id: review-result:w19-u4-adversary-3
kind: review-result
status: active
title: U4 state move, adversary pass 3
relations:
- reviews: story:state-moves-to-postgres
revision: 1
---
needs-revision

```
unit: wave 19 U4 story:state-moves-to-postgres, unit/w19-state-move at 0f2a6b79, uncommitted test files only
verdict: NEEDS-CHANGE (1 warning, 1 note)
cases: executed 48→55 (four slices of task check:postgres), red 2
origin: introduced 2 / pre-existing 0 / undecided 0
```

Cases added in `crates/spindle-cli/tests/spindle-cli/adversary_state_move.rs` (pass 3 section) and `crates/spindle-kernel/tests/spindle-kernel/adversary_state_move.rs`:

- red: `a_refused_move_into_an_unmigrated_loopback_schema_creates_no_tables`: a refused move into a loopback database without `store migrate` leaves 36 tables where 12 were.
- red: `a_move_with_a_serving_role_limited_to_the_stores_share_moves_the_directory`: role CONNECTION LIMIT 3 (3 components × `database_pool` 1), `run` starts, the move exits 1 with "store: event store resource budget exhausted".
- green: refused moves leave every table's row count and `pg_stat_user_tables` write counters unchanged; losing the lock's session mid-copy exits 1; SIGKILL and SIGTERM release the schema; a 600-group log rebuilt from the 0.10.0 `commit_all` source and a log from before 0.7 answer every scan and load as the file does after the move.

Suite: kernel `postgres::` 17 passed; kernel `state_move::` 6 passed; CLI `postgres::` 12 passed; CLI `state_move::` 18 passed, 2 failed (the red cases). Default `cargo test -p spindle-cli`: 335 passed, 3 failed (known `ui/dist`), 31 ignored.

Pass-2 findings both fixed. Measured: one component, 1,400 resting rows, 153 MB state directory written by the 0.10.0 release binary and this branch: move exit 0 in 403.9 s (debug build, host load 50–60), peak RSS 64 MiB, no mark, no schema-1 `spindle.written` left; list view 1,400 rows on both stores, 40.7 s directory, 18.9 s PostgreSQL.

```findings
- file: crates/spindle-kernel/src/state_move.rs
  line: 335
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the lock's own connection plus one idle connection per store needs components+1 connections, so a serving role whose CONNECTION LIMIT is components x database_pool 1 (admitted by run and by the documented range) cannot move, refused with the opaque "event store resource budget exhausted"
- file: crates/spindle-cli/src/state.rs
  line: 79
  category: acceptance
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: every target is opened before the emptiness check, and on a loopback database without store migrate opening creates the tables, so a refused move creates every other prefix's tables
```

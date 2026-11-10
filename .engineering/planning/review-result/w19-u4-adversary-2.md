---
format: aep.planning-md/3
id: review-result:w19-u4-adversary-2
kind: review-result
status: active
title: Wave 19 U4 adversary pass 2
relations:
- reviews: story:state-moves-to-postgres
revision: 1
---
needs-revision

```
unit: wave 19 U4 story:state-moves-to-postgres, 47c3fd85 (fix pass 1 on ba1f9219), uncommitted test file only
verdict: CONFIRMED (2 notes)
cases: executed 8→11 (state_move:: slice of task check:postgres), red 2
origin: introduced 2 / pre-existing 0 / undecided 0
```

Both pass-1 findings are fixed; their cases are green.

Cases added in `crates/spindle-cli/tests/spindle-cli/adversary_state_move.rs`:

- `a_refused_move_writes_no_row_into_any_prefix`: red. A refused move leaves one `<prefix>_identity` row per component: `(events, identity rows)` before `[(0, 0), (0, 0), (3, 0)]`, after `[(0, 1), (0, 1), (3, 1)]`.
- `two_moves_at_once_into_one_empty_database_leave_one_directory_or_none`: red, 4 of 4 runs. With the timing forced through rally-table's publication gate, both moves pass the emptiness check; the one that exits 0 leaves events of the other directory (`[3, 0, 3]` against `[0, 0, 3]`).
- `an_escaped_or_identity_only_nul_and_extreme_numbers_move_and_scan_alike`: green.

Suite: kernel `postgres::` 12 passed; CLI `postgres::` 12 passed; CLI `state_move::` 9 passed, 2 failed (the two above), 8 passed without the new cases; `cargo test -p spindle-cli` 335 passed, 3 failed (known ui/dist), 22 ignored.

Attacked and not broken: pass-1 findings 1 and 2; every text column the move writes goes through eventlog's printable-ASCII check in the first pass; raw NUL or invalid UTF-8 in SQLite JSON fails the first pass before connecting; extreme numbers round-trip; first-pass memory holds one append group at a time; the first pass runs before any connection; U1's scan after the shared `capture` refactor.

```findings
- file: crates/spindle-kernel/src/state_move.rs
  line: 162
  category: acceptance
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a refused move's emptiness check records a tenant identity row in every component's <prefix>_identity table, so "exits 1 before writing to any prefix" and the help's "writing nothing" do not hold at the row level
- file: crates/spindle-cli/src/state.rs
  line: 75
  category: concurrency
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the emptiness check and the copy are not serialised against another move, so two moves started at once both pass and the one that exits 0 leaves another directory's streams in the database
```

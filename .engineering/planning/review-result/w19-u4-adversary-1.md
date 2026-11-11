---
format: aep.planning-md/3
id: review-result:w19-u4-adversary-1
kind: review-result
status: active
title: Wave 19 U4 adversary pass 1
relations:
- reviews: story:state-moves-to-postgres
revision: 1
---
needs-revision

```
unit: wave 19 U4 story:state-moves-to-postgres, commit 955c6615 (uncommitted test files only)
verdict: NEEDS-CHANGE
cases: executed 4→7 (state_move:: slice of task check:postgres), red 2
origin: introduced 2 / pre-existing 1 / undecided 0
```

Cases added in `crates/spindle-cli/tests/spindle-cli/adversary_state_move.rs`, ignored without `SPINDLE_TEST_DATABASE_URL`:

- `a_non_empty_prefix_refuses_the_move_while_another_transaction_is_open`: red. With another client's write transaction open, a prefix holding a stream reads as empty; the move writes rally-player-east and rally-player-west (3 events each) and fails on rally-table with "stream is at version 1, not 0".
- `a_row_holding_a_nul_character_moves_or_the_move_writes_nothing`: red. A row holding U+0000, which the SQLite store keeps, fails the PostgreSQL append with SQLSTATE 22P05 after two components are written; the next move refuses the partial copy.
- `a_long_log_with_groups_across_pages_arrives_whole`: green. About 5,900 events, groups across 1000-event pages, one group of 1,401 events, a removal then re-creation: the PostgreSQL scan equals the SQLite scan, 30.7 s.

Suite: `state_move::` with the new file 5 passed, 2 failed; without it 4 passed. Default spindle-cli 333 passed, 3 failed (known ui/dist), 16 ignored.

Attacked and not broken: page and group boundaries; a missing, corrupt or foreign SQLite file and an admission refusal exit before any connection; an owner role is refused before the emptiness check; no password printed; grouping by request_id; other tenants; extra .sqlite files; views and metrics after the move.

```findings
- file: crates/spindle-kernel/src/state_move.rs
  line: 94
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: is_empty reads the watermarked feed, so while any write transaction is open on the server a prefix holding streams reads as empty and the move writes other prefixes before failing, breaking "exits 1 before writing to any prefix"
- file: crates/spindle-cli/src/state.rs
  line: 80
  category: boundary
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a row or event holding U+0000, which the SQLite store keeps, fails the PostgreSQL append (22P05) after earlier components and groups are written, leaving a partial copy no later move can complete
- file: .engineering/planning/story/state-moves-to-postgres.md
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: the acceptance's "version the directory's last write left plus one" holds only for a command emitting no event; a commit appends its events plus spindle.row, which the code and the unit's test (+2) handle correctly
```

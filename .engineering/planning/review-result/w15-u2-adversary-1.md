---
format: aep.planning-md/3
id: review-result:w15-u2-adversary-1
kind: review-result
status: active
title: Adversary pass 1 on kernel-set-outcomes (wave 15)
relations:
- reviews: story:kernel-set-outcomes
revision: 1
---
needs-change

Adversary pass 1 on story:kernel-set-outcomes (wave 15 U2), attacking 202e2db7 and 025db740. Six
cases in `crates/spindle-kernel/tests/spindle-kernel/adversary_set_outcomes.rs` (commit 2dbe2323);
the `set_outcomes::` filter went from 6 to 12 cases, 4 red.

- F1, blocker, in scope, fixed in the unit (25bc889d): a `{generated: true}` in a set outcome's `sets:` or in a
  per-row event took one value for every row (`crates/spindle-kernel/src/mint.rs:209`); ESS's
  interpreter mints one per row and per event.
- F2, warning, fixed in the unit (25bc889d, `Store::commit_all`; atomic in the memory, SQLite and IndexedDB stores, best effort in the default for other stores): each row was its own store write
  (`crates/spindle-kernel/src/execute/set.rs:96`), so a failure on a later row left earlier rows
  written, against `execute`'s promise that a command that does not run leaves the store unchanged.
- F3, warning, fixed in the unit with F2 (25bc889d): after such a failure the moved rows' per-row events were
  neither recorded nor returned, and a retry no longer selects those rows.
- F4, note: with zero rows selected the outcome's events are returned but not recorded in the
  store, as for outcomes without a subject.

Could not break: integer identities in numeric order, `when_state:` on per-row events,
`{changed: state}` and the count, a move skipping rows outside its `from` states, an invariant
failure on any row caught before any write.

```findings
- file: crates/spindle-kernel/src/mint.rs
  line: 209
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a generated value in a set outcome's sets or per-row event takes one value for every row
- file: crates/spindle-kernel/src/execute/set.rs
  line: 96
  category: concurrency
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a failed write on a later row leaves the earlier rows written
- file: crates/spindle-kernel/src/execute/set.rs
  line: 96
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: after such a failure the moved rows' per-row events are lost
- file: crates/spindle-kernel/src/execute/set.rs
  line: 96
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: with zero rows selected the outcome's events are returned but not recorded in the store
```

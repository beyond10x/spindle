---
format: aep.planning-md/3
id: review-result:wave34-adversary-U3-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 34 U3
relations:
- reviews: story:reset-on-a-busy-store-keeps-the-store
revision: 1
---
unit: U3 `story:reset-on-a-busy-store-keeps-the-store`, findings cover head 9a1e1d89 (implementation at 4004be2b)
verdict: CONFIRMED
cases: executed 518→523, red 1
origin: introduced 0 / pre-existing 1 / undecided 0
wrote-outside-worktree: 6 paths (listed in part 6)
needs-coordinator: none

The unit's own diff holds up. The one red case shows a gap that is already at the base: a reset with a world whose first store read is busy answers `400`, not the story's `503`.

**1. Diff stat** (`git --no-pager diff --stat 4004be2b HEAD`). Only test files are in it:
```
 .../tests/spindle-host/adversary_w34_u3.rs         | 258 ++++++++++++++++++++
 crates/spindle-host/tests/spindle-host/main.rs     |   2 +
 .../tests/spindle-ingress/adversary_w34_u3.rs      | 269 +++++++++++++++++++++
 .../spindle-ingress/tests/spindle-ingress/main.rs  |   1 +
```
Committed as 9a1e1d89 with `b10x-gates bot -- commit`. The check for private strings printed nothing.

**2. Cases added**

`crates/spindle-host/tests/spindle-host/adversary_w34_u3.rs`, both green and kept:
- `a_reset_whose_removal_conflicts_still_loses_the_store`: a removal refused with `StoreError::Conflict` returns `HostError::Reset { restored: false }`, and later commands report the store as "lost in a reset". This catches a mutant the unit's tests miss: if the code kept the store on any error except `Backend`, the unit's tests would stay green because they only test `Backend`. That mutant was not run.
- `a_busy_step_at_any_position_keeps_the_store_and_counts_what_landed`: covers 1 to 3 rows, busy at every removal and at the reset mark. It checks the busy error names `desk`, the step and the detail; `reset_changed_rows() == (k > 0)`; the rows left are `n - k`; a command still commits; and sending the reset again empties the store.

`crates/spindle-ingress/tests/spindle-ingress/adversary_w34_u3.rs`:
- `a_world_row_held_busy_after_the_rows_were_removed_answers_500_and_keeps_the_store`: green, kept. This is the path the implementor said had no test. The answer is `500` saying "kept" and "sent again", the next command answers `opened`, and the reset sent again answers `200` and loads the world.
- `a_world_row_held_busy_on_a_store_that_held_no_row_answers_503`: green, kept. The answer is `503` with `committed: false`, as decided in D3.
- `a_reset_whose_world_check_reads_a_busy_store_answers_503_not_400`: **red**. Its output when run alone, before the suite:
```
panicked at crates/spindle-ingress/tests/spindle-ingress/adversary_w34_u3.rs:260:5:
assertion `left == right` failed: a reset held by a busy store before it changed a row answers the documented 503: {"refused":"the world was refused: row 0: desk's store cannot hold study.desk.Desk \"desk-9\": store: event store deadline exceeded during database statement or lock"}
  left: (400, Null)
 right: (503, Bool(false))
Summary [   0.009s] 1 test run: 0 passed, 1 failed, 231 skipped
```
Every new case takes under 0.03 s.

**3. Suite run, after the cases existed**
- `cargo nextest run --locked -p spindle-host -p spindle-ingress --no-fail-fast` gave `523 tests run: 522 passed, 1 failed, 14 skipped` and exit 100. The one failure is the red case above.
- The same command with `-E 'not test(/^adversary_w34_u3::/)'` gave `518 tests run: 518 passed, 19 skipped` and exit 0.
- `cargo clippy --locked -p spindle-host -p spindle-ingress --all-targets -- -D warnings` finished clean.
- `rustfmt --check` on both new files is clean. I formatted them through `slot --fetch`.

**4. Findings**
- `crates/spindle-host/src/host.rs:1913`, read together with `crates/spindle-ingress/src/console.rs:598`.
  - **What was measured:** `Host::reset_to` reads each world row from its store before anything is emptied. It turns any error from that read into `HostError::World`, so a `Busy` store answers `400` "the world was refused … store cannot hold". Nothing has changed at that point, so the story's Outcome ("while the reset has changed no row, it answers `503`") expects `503`.
  - **Origin:** pre-existing. The same `map_err` and the same `400` arm are at 2577c62b, read with `git show`.
  - **What reaches it:** a `POST /api/reset` with a world on PostgreSQL whose read runs past its statement or store-operation deadline. Plain stream reads take no lock, so this comes from a slow database, not from the lock wait in the story. The wave 33 suite already tests busy loads as a real case for commands (`Failing::LoadsBusy`).
  - **Fix (not applied):** answer a `Busy` error from that read as `HostError::Store(StoreError::Busy(..))` instead of `World`.

**5. Attacked and could not break**
- The claim "Busy means nothing began to commit", checked by reading eventlog 0.10.0: a COMMIT error becomes `UnknownCommit`, and a timeout after COMMIT starts becomes `UnknownCommit` too (`timed_out_write`). So `Deadline`, and therefore `Busy`, only comes before COMMIT. I did not add a PostgreSQL lane case.
- The `503`/`500` split when a removal is busy, the order of the `reset_changed_rows` guard, and the catch-all arms: neither match is exhaustive, so U2's new variant can still be added.
- The seed put-back in `serve.rs`: `restore` is true only for a non-200 answer when no rows changed, and that matches the new `503`.
- Counted row counts after a partial removal, and the reader gate after a busy return: `publish` still runs.
- Documents and the CHANGELOG against the code.

**6. Paths written outside the worktree**
In `<local path>`:
- `red-ingress.log`
- `host-cases.log`
- `ingress-cases.log`
- `suite-after.log`
- `suite-before.log`
- `commit-msg.txt`

**7.**
```findings
- file: crates/spindle-host/src/host.rs
  line: 1913
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: a reset with a world whose pre-empty row read is busy answers 400 "the world was refused" instead of the 503 committed:false the story's Outcome gives a busy reset that changed no row
```

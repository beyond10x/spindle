---
format: aep.planning-md/3
id: review-result:wave33-adversary-U2-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 33 U2
relations:
- reviews: story:new-leader-write-while-old-paused
revision: 1
---
unit: story:new-leader-write-while-old-paused (U2). The findings cover commit 7d37f364c on unit/w33-busy-store (implementation at 4dbf23af).
verdict: CONFIRMED
cases: executed 210→213, red 1
origin: introduced 3 / pre-existing 1 / undecided 0
wrote-outside-worktree: 9 paths (part 6)
needs-coordinator: none

**1. `git --no-pager diff --stat 4dbf23af..HEAD`**
```
 .../tests/spindle-ingress/adversary_w33_u2.rs      | 309 +++++++++++++++++++++
 .../spindle-ingress/tests/spindle-ingress/main.rs  |   1 +
 2 files changed, 310 insertions(+)
```
Only test files changed. Commit `7d37f364c`, made with `b10x-gates bot -- commit`. The check for names and home paths printed nothing.

**2. Cases added**

All three are in `<local path>`. Each uses a stub store over the `desk` fixture and runs in under 1 s.
- `an_events_read_held_by_a_busy_store_answers_the_documented_501`: **red now.** The store's `events_after` returns `StoreError::Busy`. The case asserts the `501` that `capabilities.md:242` and the CHANGELOG promise for an events read held behind the lock. This is the run of the case alone, before the suite:
  ```
  assertion `left == right` failed: capabilities.md says a held events read answers 501: Some(Object {"refused": String("store: event store deadline exceeded during database statement or lock")})
    left: 500
   right: 501
  test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 190 filtered out
  ```
  The first attempt went through the listener and was red for the wrong reason: 404, because that surface serves no `/api/events`. I rewrote it to call `console::events` directly, and the run above is the rewritten case.
- `a_write_the_store_failed_otherwise_still_answers_501`: **green.** A commit that fails with `StoreError::Backend` answers `501` `{"refused":"store: …","committed":false}`, the request log records it as `not-served`, and nothing is written. No other test pins this. It catches the mutant `ExecuteError::Store(_) => Answer::busy(..)` at `answer.rs:584`, which `busy_store.rs` lets through. I worked this out from the code; I did not build the mutant.
- `a_command_whose_read_is_busy_answers_503_and_writes_nothing`: **green.** A `Busy` from `load` inside `TakeDesk` answers `503` with `committed:false`, the log records it as `failed`, and the store is asked for no write.

**3. Suite run, after the cases existed**
- `cargo test --locked -p spindle-ingress --no-fail-fast` exited 101: `test result: FAILED. 190 passed; 1 failed` plus `22 passed` in the lib tests. The red case is the only failure.
- The same command with `-- --skip adversary_w33_u2` exited 0: `188 passed … 3 filtered out` plus `22 passed`. That run gives the before count of 210.
- `cargo clippy --locked -p spindle-ingress --all-targets -- -D warnings` exited 0.
- `cargo fmt -p spindle-ingress --check` exited 0.
- I did not run the PostgreSQL lane.

**4. Findings**

| file:line | verdict | origin | finding | what reaches it |
|---|---|---|---|---|
| `website/docs/concepts/capabilities.md:242` (also `CHANGELOG.md:82`) | CONFIRMED | introduced | The docs say a view or events read held behind the lock "still answers `501`". `GET /api/events` answers `500` (`console.rs:482`/`:490`); that code was already there at the base. Fix: change the doc line to say events reads answer `500`, or change the code to `501`. | A `GET /api/events` on a PostgreSQL leader during the window. `LogStore::events_after` reads the feed, `publication_gate` takes the lock exclusive and fails with 55P03, eventlog returns `Deadline`, which becomes `Busy`, then `HostError::Store`, then `500`. |
| `crates/spindle-ingress/src/answer.rs:584` | CONFIRMED | introduced | Before my case, no test checked that a store failure other than `Busy` stays at `501`. My green case now covers it. | Any command whose commit fails with `Backend`, for example a lost connection. |
| `CHANGELOG.md:77` | INFEASIBLE | introduced | The CHANGELOG says "on PostgreSQL and SQLite alike". eventlog-sqlite 0.10.0 never returns `Deadline`: its `backend` (`lib.rs:4103`) maps every rusqlite error, SQLITE_BUSY included, to `Backend`. A SQLite write waiting on another connection's lock therefore answers `501`. | Nothing in Spindle reaches it: `state_lock.rs` allows one process per state, and WAL mode lets reads run beside writes. Only an outside process writing the file could. |
| `crates/spindle-kernel/src/sqlite.rs:2966` | CONFIRMED | pre-existing | When the commit outcome is unknown (`UnknownCommit`: eventlog's deadline fired after COMMIT was sent), the request answers `501` with `committed:false`, though the write may have landed. | A commit slower than the 10 s store deadline after COMMIT was sent. The publication-lock window does not reach it: those writes fail before COMMIT. |

**5. What I attacked and could not break**
- **Can a `503` hide a write that landed?** No. A command makes one append (`commit`, `commit_all` and `commit_events` each make one append group). eventlog sets `commit_started` before COMMIT, and from then on a failed or timed-out COMMIT returns `UnknownCommit`, never `Deadline`. The eventlog calls that can return `Deadline` after a commit (`put_blob`, `run_catch_up`, `save_snapshot_checked`) are never called on the command path.
- **Errors after the commit:** they reach the ingress as other `HostError` variants and answer `501` with `committed:true`. `HostError::Execute` wraps only the submitted command's own `execute`.
- **Lease check:** `serve_with_unless` still treats a `503` with `committed:false` as uncommitted, so the lease-lost check still runs.
- **Other code matching `StoreError`:** none of it now misses `Busy`. `state_move.rs:224` matches only the `Overloaded` text; `establish.rs:52` matches `Conflict`.
- **Consumer contract:** the request log class (`failed`), the metrics kind (`store`), the `cli.rs` help and the `environment-and-admission.md` text all match the code. The 2 s lock wait and 10 s idle timeout in the docs match eventlog's defaults.
- **The unit's own tests:** in the kernel lane test the identities match and "appends nothing" is a real check (scan version 0 and load `None`). The w31 retry loop would fail on `LEASE_LOST`, because that body has no `committed` field.
- **Process note:** I did not take a worktree lease for this pass.

**6. Paths written outside the worktree**, all under `<local path>`:
- `head.rs`, `tail.rs`, `body.rs`, `events.rs`, `commit-msg.txt`
- `suite.log`, `suite-before.log`, `clippy.log`, `fmt.log`

**7.**
```findings
- file: website/docs/concepts/capabilities.md
  line: 242
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the page and the CHANGELOG say an events read held behind the publication lock answers 501, but GET /api/events over a Busy store answers 500 (console.rs paged/answered)
- file: crates/spindle-ingress/src/answer.rs
  line: 584
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: no test pinned that a non-Busy store failure of a command stays 501, so answering every ExecuteError::Store 503 passed the unit's suite; the kept green case now covers it
- file: CHANGELOG.md
  line: 77
  category: judgement
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: '"on PostgreSQL and SQLite alike" claims a SQLite 503 that cannot occur, because eventlog-sqlite 0.10.0 never returns Deadline and a SQLITE_BUSY write answers 501'
- file: crates/spindle-kernel/src/sqlite.rs
  line: 2966
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: UnknownCommit maps to Backend and the ingress answers 501 with committed false for a write whose commit may have landed
```

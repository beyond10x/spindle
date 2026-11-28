---
format: aep.planning-md/3
id: review-result:wave33-adversary-U2-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 33 U2
relations:
- reviews: story:new-leader-write-while-old-paused
revision: 1
---
unit: story:new-leader-write-while-old-paused, head 1b4ce52e5 (findings cover 65182532 plus my test commit)
verdict: CONFIRMED
cases: executed 210→216, red 1
origin: introduced 1 / pre-existing 1 / undecided 0
wrote-outside-worktree: 7 paths (part 6)
needs-coordinator: decide whether "a write" in environment-and-admission.md § Several processes on one store includes `POST /api/reset`. If it does, the code is wrong. If it does not, the sentence needs narrowing. The lost store after a failed reset existed before this unit and needs its own backlog item.

**Summary.** The read statuses the revised docs give hold on both read paths: on the host's thread and through the reader. I rewrote the round-1 case so it asserts the documented 500, and it is green. The PostgreSQL lane tests have no wrong-reason pass that I could find. One new red case: the docs added by this unit say a write held behind a paused leader answers 503, can be sent again, and the new leader keeps leading. `POST /api/reset` held the same way answers 500 instead, and replaces the component's store with a lost one. After that, every command answers 501 until the run restarts.

**1. `git --no-pager diff --stat 65182532 HEAD`**
```
 .../tests/spindle-ingress/adversary_w33_u2.rs      | 308 +++++++++++++++++++--
 1 file changed, 285 insertions(+), 23 deletions(-)
```
This is a test file only. No implementation file was touched.

**2. Cases** in `<local path>`. Each runs in under 0.02 s.
- `an_events_read_held_by_a_busy_store_answers_the_documented_500` (renamed from `…_documented_501`): asserts `500 {"refused":"store: <deadline>"}`. **Green.**
- `reads_held_on_the_hosts_thread_answer_the_documented_statuses` (new): sends five reads to a store whose every read is busy. The view must answer `501 {"refused"}`. `GET /api/events` (whole, `?limit=1`, `?after=head`) and `GET /api/history` must answer `500 {"refused"}`. The body must be exactly that, with no `committed`, and nothing may be written. **Green.**
- `reads_held_through_the_reader_answer_the_documented_statuses` (new): the same reads through `Listener::bind_reading`, with a busy reading handle that counts its calls (more than 0, so the reader path ran). **Green.**
- `a_reset_held_by_a_busy_store_answers_the_documented_503_and_the_run_keeps_its_store` (new): while the store's writes, including the reset mark (`forget_events`), are busy, `POST /api/reset` must answer 503 with `committed: false`. Once the store answers again, `OpenDesk` must answer 202. **Red.** Output of that run (the module alone, before the suite):
```
assertion `left == right` failed: the reset answered 500 {"refused":"resetting desk: forgetting its events: store: event store deadline exceeded during database statement or lock; its store could not be reopened, and every read and write of it now fails"}; the command after it, the store answering again, answered 501 {"committed":false,"refused":"store: desk's store was lost in a reset: forgetting its events: store: event store deadline exceeded during database statement or lock"}
  left: (500, Null)
 right: (503, Bool(false))
```
The two round-1 cases are unchanged and green: `a_write_the_store_failed_otherwise_still_answers_501` and `a_command_whose_read_is_busy_answers_503_and_writes_nothing`.

Two earlier runs of the module were broken setup, not findings: the console was not enabled (404), and I expected 201 where the route answers 202. I fixed both before the run quoted above.

**3. Suite**, run after the cases existed:
- `cargo test --locked -p spindle-ingress --no-fail-fast` gave `193 passed; 1 failed` and `22 passed` (exit 101). The only failure is the reset case.
- The same command with `-- --skip adversary_w33_u2` gave `188 passed; 6 filtered out` and `22 passed` (exit 0). That gives the before count of 210.
- `cargo fmt -p spindle-ingress --check` exited 0. `cargo clippy --locked -p spindle-ingress --all-targets -- -D warnings` exited 0.
- I did not run the PostgreSQL lane. My analysis of it is in part 5.

**4. Findings**

| file:line | finding | verdict | origin |
|---|---|---|---|
| `website/docs/concepts/environment-and-admission.md:635` | The text says "A write to the new leader held behind them answers `503` … safe to send again, and the new leader keeps leading". `POST /api/reset` held the same way answers `500 {"refused"}` (`console.rs` `reset`) and loses the store. | CONFIRMED | introduced (the sentence is new; at the base this section made no status promise) |
| `crates/spindle-host/src/host.rs:1957` | A reset of a supplied store, which is what PostgreSQL is, replaces the store with `Lost` when its first removal or reset mark fails, including on `StoreError::Busy`. After that, every command answers 501 until the run restarts. | CONFIRMED | pre-existing (`spindle-host` is unchanged since 3a5ec126) |

How a real run gets there: a leased PostgreSQL run without a seed sends `POST /api/reset` to `console::reset` (`serve.rs`, leased branch). The console is on by default on loopback. `LogStore::forget_events` and `remove` go through eventlog's `append_guarded`, which takes the publication lock shared (`eventlog-postgres` lib.rs:438). So a reset in the paused-leader window waits out its 2 s lock wait and gets the deadline error. My stub builds that state; I did not reproduce it on PostgreSQL.

Possible fixes, which I did not apply: narrow the sentence to "a command", or map a reset's busy failure to 503 without losing the store.

**5. Attacked without a break**
- Read statuses against the docs (view 501; events whole, ranged and at its head 500; history 500) hold on both paths.
- The SQLite claim holds: eventlog-sqlite 0.10.0 never returns `Deadline`.
- `committed: false` holds: on the append path eventlog turns a deadline after COMMIT into `UnknownCommit`. In the kernel, `take` commits last.
- In the replicas test and both `adversary_w31_u6` rewrites, `503` with `committed: false` comes only from `Answer::busy`. A lost lease answers `refusal(503, LEASE_LOST)` with no `committed` field, so the `committed` assertions reject it.
- A wrong advisory key in those tests would not block the write; the test would see 201 and go red, not green.
- All four new lane tests match the `postgres::` filter, so the lane runs them.
- `serve_with_unless` treats the busy 503 as uncommitted (`status >= 500` and `committed == false`), so a busy 503 still triggers the lease check.
- `a_commit_behind_the_publication_lock_is_busy`: a wrong key would make it red. Its "appends nothing" check reads after the holding session ends.
- Mutants caught: a missing `backend` mapping, a changed display of the error, or `Overloaded` mapped to busy (unit test); a body without `committed` (busy_store); every store error mapped to 503 (round-1 case).

**6. Paths written outside the worktree**, all under `<local path>`:
- `cases-alone.log`
- `cases-alone-2.log`
- `cases-alone-3.log`
- `suite.sh`
- `suite-after.log`
- `suite-before.log`
- `commit-msg.txt`

`fmt.log` and `clippy.log` are also in that directory.

**7.**
```findings
- file: website/docs/concepts/environment-and-admission.md
  line: 635
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the new sentence promises that a write held behind a paused leader answers 503 and the leader keeps leading, but POST /api/reset held so answers 500 and loses the store (adversary_w33_u2::a_reset_held_by_a_busy_store_answers_the_documented_503_and_the_run_keeps_its_store, red)
- file: crates/spindle-host/src/host.rs
  line: 1957
  category: judgement
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: a reset of a supplied (PostgreSQL) store whose removal or reset mark fails, busy included, replaces the store with Lost, so every later command answers 501 until the run restarts
```

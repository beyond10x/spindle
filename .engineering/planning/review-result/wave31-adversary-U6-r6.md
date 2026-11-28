---
format: aep.planning-md/3
id: review-result:wave31-adversary-U6-r6
kind: review-result
status: active
title: Adversary, round 6, on wave 31 U6
relations:
- reviews: story:several-writers-one-postgres
revision: 1
---
unit: story:several-writers-one-postgres, round 5, head f503be99 (67689beb plus my test commit)
verdict: CONFIRMED
cases: executed 135→139, red 1
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 5 new paths in the U6 adv-scratch directory (part 6)
needs-coordinator: none

The new cut bounds every store operation by the lease, reads included, not only transactions as the docs say. On a 1 s lease, a read held up 1.5 s gets a 501 after 1 s. A write longer than the lease is refused whole and loses the leader its lease. Writes within the lease land. The warning fires once per change of error. I found no other path that holds a lease-row lock across a pause.

**1. Diff stat (67689beb..HEAD).** One test file, no other path.
```
 .../tests/spindle-cli/adversary_w31_u6.rs          | 207 +++++++++++++++++++++
```

**2. Cases added** (module `adversary_w31_u6::postgres`, run alone in the lane before the suite)
- **`a_read_held_up_longer_than_a_short_lease_answers`: red.** On a 1 s lease, board's event table is locked `ACCESS EXCLUSIVE` for 1.5 s while `GET /tasks` is read. Expected: 200 once the lock ends. Output:
  ```
  the read held up 1.5 s on a 1 s lease answered 501 after 1.002444164s
  assertion `left == right` failed: {"refused":"store: event store deadline exceeded during store operation"}
    left: 501
   right: 200
  ```
- **`a_write_longer_than_a_short_lease_lands_whole_or_not_at_all`: green.** On a 1 s lease, inserts into board's event table are blocked for 1.5 s while a command is sent.
  - What the caller gets: 501 after 1.0 s, `{"committed":false,"refused":"store: event store deadline exceeded during store operation"}`.
  - Nothing is half-committed: `t2` is not held, and `t1` is.
  - The leader does not keep leading. It loses the lease and the standby takes over.
- **`a_write_held_up_within_the_lease_lands_and_keeps_the_leader`: green.** On a 2 s lease the write is held up 1.2 s: it gets 201, the leader keeps leading, and the standby does not take over.
- **`a_refused_standby_warns_once_and_says_when_it_is_back`: green.** The standby's role is set to `NOLOGIN` for 4 s, about six polls.
  - It prints exactly two warnings, one per distinct error: "connection closed", then `role … is not permitted to log in (SQLSTATE 28000)`.
  - When the role is given `LOGIN` again, it prints one "is made again" line and no further warning.
- My round-4 red case `a_standby_the_database_refuses_says_so` is now green (your F1).

**3. Suite, run after the cases existed**
- `slot --timeout 30m sh ./.pglane.run.sh task check:postgres`: `Summary [44.346s] 139 tests run: 138 passed, 1 failed, 1463 skipped`. Within reports `44.876 s of 120 s`; the harness exited 201. The one failure is the red case above.
- "Before" is 135: the same run without my 4 round-5 cases.
- The slowest tests are the kernel scan adversaries (19 to 27 s on this node load). No U6 test is among them.
- `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings`: exit 0.
- `cargo fmt -p spindle-cli --check`: exit 0.

**4. Findings**

**`crates/spindle-kernel/src/postgres.rs:189`: contract drift, note, CONFIRMED, introduced.**
- What happens: `options.transaction_timeout.min(leased.term)` also shortens eventlog's deadline on every store operation. Every read, feed read and capture is wrapped in `bounded()`, which uses that deadline. So under a lease shorter than 10 s, a read slower than the lease fails with 501. The docs say only that "a transaction … lasts at most `database_lease`".
- What reaches it: a lease under 10 s together with a slow read, such as a long scan, a held table lock, or a loaded database. The lane runs its lease tests at 1 to 2 s. At the default 10 s lease nothing changes, since min(10, 10) = 10.
- Possible fix: either cut only `idle_in_transaction_session_timeout` to the lease and keep the operation deadline, or document that every store operation is bounded by the lease.

What the transaction cut does, answering your three questions:
- **What the caller gets:** a write slower than the lease gets 501 `event store deadline exceeded` after one lease, not 503.
- **Half-commits:** none; the write is refused whole.
- **Does the leader keep leading:** no, it loses its lease.
  - Cause: the cancelled write's server-side statement keeps the lease row locked until its own lock wait ends. The idle-in-transaction timeout does not cut an active statement.
  - That blocks renewals past the lease, so the standby takes over.
  - This loss already happened before round 5, because the fence holds the lease row for the whole write. Round 5 only changes what the caller is told.

**5. Attacked and could not break**
- A write held up within the lease lands and keeps its leader.
- A write held up longer than the lease is never half-committed.
- The warning is once per change, with one "made again" line.
- Other paths that could hold a lease-row lock across a pause:
  - The take, renew and release statements are single autocommit statements.
  - The cursor saves and dead letters go through fenced stores, opened after the lease is taken, so they carry the lease-term cut.
  - Only an active statement outlives the idle cut. The append transaction runs none that streams large results, so I found no path that reaches it.

**6. Paths written outside the worktree** (all under `<local path>`)
- `r5-clippy.log`
- `r5-run1.log`
- `r5-lane.log`
- `commit6.txt`
- `r4-run3.log` (from the round-4 rerun)

The files from earlier rounds are still there. The two roles the red cases created stay in the throwaway lane cluster, which is removed after each run.

**7. Findings block**
```findings
- file: crates/spindle-kernel/src/postgres.rs
  line: 189
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the cut of the transaction timeout to the lease also cuts eventlog's deadline on every store operation, so under a lease shorter than 10 s a read held up longer than the lease is answered 501 deadline exceeded, where the documents bound only a transaction
```

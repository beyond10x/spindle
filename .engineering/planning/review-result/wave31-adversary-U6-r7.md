---
format: aep.planning-md/3
id: review-result:wave31-adversary-U6-r7
kind: review-result
status: active
title: Adversary, round 7, on wave 31 U6
relations:
- reviews: story:several-writers-one-postgres
revision: 1
---
Round 6 is red: 3 of the 6 new cases fail. The ending does end a live leader's write and answers it 501. It answers 501 instead of 503 for a paused leader's in-flight command. Two share lockers on an expired lease row leave the standby waiting with no line at all. No case found it ending the wrong session (another component's lease, an unrelated session, or the new leader after a failover). The cases are committed as `4e009a39` on top of `51421b52`.

```
unit: U6 wave 31, story:several-writers-one-postgres, adversary round 6, head 51421b52 -> 4e009a39
verdict: red
cases executed: 141 -> 147 (lane `task check:postgres`)
red: 3
origins: introduced 2, pre-existing 1, undecided 1
wrote outside worktree: scratch only; needs coordinator: yes (decide F1)
```

**Diff stat**
`crates/spindle-cli/tests/spindle-cli/adversary_w31_u6.rs`: +502 lines, 1 file. No other file touched. `.pglane.*` was removed before the commit.

**Cases** (all PostgreSQL lane, all on a 2 s lease unless stated; new processes run with a pool of 2)
1. `a_live_leaders_write_held_up_past_a_short_lease_is_not_ended` (line 1723): **red**.
   - Set-up: 1 s lease; inserts into board's events are blocked for 1.5 s while the leader writes `t2`.
   - Result: `t2 answered 501 after 997ms: {"committed":false,"refused":"store: event store is unavailable: PostgreSQL connection unavailable"}; the old leader leads on; held ["t1"]`.
   - The standby printed `the lease of board ran out while a process still held its row: ended 1 of its database sessions`.
2. `a_leader_whose_write_a_standby_ended_answers_consistently_and_writes_on` (1768): green.
   - The ended write is whole (answered 501, not held).
   - The old leader still leads, and its next 3 commands are 201: the pool does not hand out the ended session again.
3. `a_leader_paused_inside_a_write_answers_that_command_503_and_lands_nothing` (1818): **red**.
   - Result: `("f24", 501)`, not held, old leader `exited Some(1)`, every earlier 201 is held.
   - Same 501 in all 3 runs (f12, f24 and the full-lane run).
4. `a_lease_row_two_sessions_share_past_its_end_is_not_waited_on_in_silence` (1888): **red**.
   - The lease row's xmax is a multixact id (`"1 multi true"`), so the ending matches no session.
   - 4 s past the lease's end: `holders alive: (true, true)`, and the standby printed only its standing-by line.
   - No misfire: a bystander session holding a lock on the lease table and a transaction id of its own was not ended.
   - Once the holders commit, the standby takes the lease.
5. `a_standby_ends_no_session_holding_another_components_running_lease` (1959): green.
   - The board holder is ended and board moves to token 2.
   - A session holding ledger's row while ledger's lease is still running, which also has a lock on board's lease table, is not ended. No "lease of ledger ran out" line.
6. `after_a_failover_the_standby_left_ends_no_session_of_the_new_leader` (2020): green.
   - 1 s lease, 3 processes, the leader killed.
   - Commands sent to the new leader for 4 s are all 201; neither process prints a "ran out" line.

**Suite run**
- Lane runs (147 cases each):
  - Run 1: 29.5 s, 4 failed (my 3 reds, plus `adversary_state_move::a_move_whose_lock_session_ends...` on `sorry, too many clients already`).
  - Run 2: 29.1 s, 5 failed (my 3 reds, plus two older U6 cases on `too many clients`).
  - Run 3, after the new cases' pool was cut to 2: 39.4 s, 4 failed. These were my 3 reds and `postgres::replicas::a_standby_posts_no_webhook_and_keeps_no_dead_letter` (`the run did not exit within 10s`). That one is not my case; it failed only in this slower run. No `too many clients` in run 3.
- Slowest test: my round-3 case `a_healthy_leader_posts_promptly_on_a_short_and_a_long_lease`, 29 to 39 s (marked SLOW over 30 s in run 3). It is under the 60 s kill, but it is the longest case in the lane.
- No new case runs over 6.2 s alone.
- `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings` is clean, and `cargo fmt -p spindle-cli --check` is clean.

**Findings**
- **F1 (medium, introduced): the ending kills a live leader's write.**
  - Where: `crates/spindle-kernel/src/postgres.rs:774` (`END`) and `:640`.
  - When a leader is not paused but its write is held up past the lease, its own renewals wait behind that write's lock on the lease row. The committed row's end time then passes, and the standby ends the write.
  - Result: answered 501, and no takeover, because the leader's waiting renewal gets the row first and keeps the lease. The ending only costs a command that landed 201 before round 6.
  - The docs (`environment-and-admission.md:596-601`) frame the ending around a paused write.
  - Possible fix: skip the ending while the session the row names is itself waiting for a lock (`pg_stat_activity.wait_event_type = 'Lock'`, which is a renewal in progress). Or document that a write held up past the lease is ended and answered 501.
- **F2 (low/medium, pre-existing, made common by round 6): wrong answer status for the in-flight command.**
  - Where: `crates/spindle-cli/src/serve.rs:754-757`.
  - Only a fence refusal is answered 503. A write whose session was ended fails with "PostgreSQL connection unavailable" and is answered 501, even though the docs (`:611`) say "answers the request it took and those already queued `503`".
  - Possible fix: after a store error under a lease, map the answer to 503 when the lease is known lost (`Kept::confirmed()` or `renewal.lost`).
- **F3 (low, introduced): two share lockers are never ended, and nothing is said.**
  - Where: `postgres.rs:774`.
  - When two sessions lock the row FOR SHARE or FOR KEY SHARE, xmax is a multixact id that no `transactionid` lock names. The standby ends nobody, prints nothing, and waits until they commit, with no time limit for sessions outside spindle.
  - The docs promise it "ends exactly the database sessions holding that row lock". A single share locker is ended.
- **F4 (low, introduced, no red case): the multixact id can collide with a live transaction id.**
  - `END` compares xmax to `pg_locks.transactionid` without checking whether xmax is a multixact.
  - In a database whose multixact counter has reached the live transaction-id range, a multixact id can equal the id of a live transaction. If that transaction's session holds a lock on the lease table (for example a leader's write waiting on the row), it would be ended instead of the real holders.
  - It cannot be built in the lane: a fresh cluster's multixact ids stay far below its transaction ids.
- **F5 (undecided, test infra): the lane is at the server's connection limit.**
  - The scratch harness runs PostgreSQL 17 with the default `max_connections` (100). At 147 cases, 2 of 3 runs failed on `sorry, too many clients already`.
  - Cutting my new cases to a pool of 2 cleared it in run 3, but the margin is thin. Raise `max_connections` in the harness and in CI, or cap the postgres profile's test threads.

**What could not be broken**
- Clock skew between processes and the server: `END` and `TAKE` compare times by `clock_timestamp()` on the server only.
- A lease renewed just as the standby reads: the committed version then carries a fresh end time, so nothing is ended.
- Another component's lease (case 5).
- An unrelated session holding a lock on the lease table (cases 4 and 5, plus the implementor's unrelated table).
- The new leader after a failover (case 6).
- The standby's own lease session: excluded by `pg_backend_pid()`.
- An ended session reused by the pool (case 2).
- A half-commit: none in cases 1 to 3.
- Not tested: a prepared (two-phase) transaction holding the row has a null pid in `pg_locks`, so it would never be ended.

**Paths written outside the worktree**
All in `<local path>`: `r6-alone.log`, `r6-alone2.log`, `r6-probe.log`, `r6-lane.log`, `r6-lane2.log`, `r6-lane3.log`, `commit7.txt`, `t3.rs`, `new.rs`.

```yaml
findings:
  - id: F1
    severity: medium
    origin: introduced
    where: crates/spindle-kernel/src/postgres.rs:774
    case: adversary_w31_u6::postgres::a_live_leaders_write_held_up_past_a_short_lease_is_not_ended
    summary: a live leader's write held up past the lease is ended by the standby, answered 501; the leader keeps the lease
  - id: F2
    severity: low
    origin: pre-existing
    where: crates/spindle-cli/src/serve.rs:754
    case: adversary_w31_u6::postgres::a_leader_paused_inside_a_write_answers_that_command_503_and_lands_nothing
    summary: the in-flight command of a leader whose write session was ended is answered 501, not 503 as the docs say
  - id: F3
    severity: low
    origin: introduced
    where: crates/spindle-kernel/src/postgres.rs:774
    case: adversary_w31_u6::postgres::a_lease_row_two_sessions_share_past_its_end_is_not_waited_on_in_silence
    summary: a multixact xmax (two share lockers) ends nobody and the standby waits with no line
  - id: F4
    severity: low
    origin: introduced
    where: crates/spindle-kernel/src/postgres.rs:774
    case: none
    summary: xmax is compared to transaction ids without checking for a multixact; a numeric collision could end a session that is not the holder
  - id: F5
    severity: low
    origin: undecided
    where: lane harness, PostgreSQL max_connections 100
    case: none
    summary: at 147 cases the lane hit "too many clients already" in 2 of 3 runs
```

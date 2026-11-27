---
format: aep.planning-md/3
id: review-result:wave31-adversary-U6-r5
kind: review-result
status: active
title: Adversary, round 5, on wave 31 U6
relations:
- reviews: story:several-writers-one-postgres
revision: 1
---
unit: story:several-writers-one-postgres, round 4, head 240d9f19 (59403241 plus my test commit)
verdict: CONFIRMED
cases: executed 132→134, red 1
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 9 new paths in the U6 adv-scratch directory (part 6)
needs-coordinator: one lane-only failure of `a_resumed_leader_posts_no_event_the_new_leader_committed` with an eventlog deadline, green 4 of 4 when run alone; cause not isolated (part 3)

Your round-4 fixes hold: a standby whose lease connection is dropped, even three times in a row, stays standing by and still takes over. One new case is red: a standby whose database refuses it from now on (its role can no longer log in) also stays standing by, but prints nothing. It looks exactly like a healthy standby, yet can never take over.

**1. Diff stat (59403241..HEAD).** One test file, no other path.
```
 .../tests/spindle-cli/adversary_w31_u6.rs          | 133 +++++++++++++++++++--
```

**2. Cases added and changed** (module `adversary_w31_u6::postgres`, run alone in the lane before the suite)
- **`a_standby_the_database_refuses_says_so`: red, in the run alone and in the lane.**
  - Setup: the standby runs as its own hosted role over TLS; the test then sets that role `NOLOGIN` and terminates its sessions.
  - Expected: within three polls the standby keeps running and prints a line about the lease connection it cannot make.
  - Output:
  ```
  a standby the database refuses says nothing, reading as a healthy standby: b10x-spindle: standing by: another process holds the lease of board, so this run serves nothing until it takes the leases of board, ledger
  ```
- **`a_standby_dropped_three_times_still_takes_over`: green.** Its lease connection is terminated three times, each time on the connection it made again. It stays standing by (503 with `standby: true`), then takes over after SIGKILL of the leader and answers 201.
  - My first version terminated faster than the standby reconnects, which only happens at the poll after the one that found the connection closed. The helper now waits for the new connection.
- **Ask 2 is done.** `posted_after` now waits 30 s, and the 5 s latency bounds are gone.
  - `a_healthy_leader_posts_promptly_on_a_short_and_a_long_lease` asserts "promptly" as: no `cannot read the store` line, meaning no read held back by the lease check. It prints the slowest post: 365 ms on a 1 s lease and 102 ms on a 10 s lease when run alone.
  - `a_shortest_lease_held_up_by_a_row_lock_keeps_leading_and_posting` gets the same 30 s wait; it printed 91 ms alone.
  - Under the lane, the healthy case took 24.8 s in total and still passed.
- **Round-3 case `a_dropped_standby_session_keeps_it_standing_by`: now green** (your F1).

**3. Suite, run after the cases existed**
- `slot --timeout 30m sh ./.pglane.run.sh task check:postgres`:
  - `Summary [24.781s] 134 tests run: 132 passed, 2 failed, 1463 skipped`; within reported `25.229 s of 120 s`.
  - "Before" is 132: the same run without my 2 round-4 cases.
  - The first failure is the red case above.
  - The second is `a_resumed_leader_posts_no_event_the_new_leader_committed`, at its assertion that the new leader answers `t2` with 201. It got `501 {"committed":false,"refused":"store: event store deadline exceeded during database statement or lock"}`.
  - That same case passed 4 of 4 runs alone, and it passed in the round-2 and round-3 lanes.
  - The lane was heavily loaded: the healthy case took 24.8 s, and `the_head_reads_no_event_of_a_long_log` took 19.0 s.
  - So I am not raising it as a finding. One untested explanation: the old leader was paused in the middle of a transaction and still held a lock. The other: lane load pushed the new leader past eventlog's 2 s lock wait.
- `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings`: exit 0.
- `cargo fmt -p spindle-cli --check`: exit 0.

**4. Findings**
**`crates/spindle-kernel/src/postgres.rs:533`, reached through `crates/spindle-cli/src/serve.rs:899`: contract drift, warning, CONFIRMED, introduced.**
- What happens: a take whose connection is closed or refused now answers "held" without a word. When the database refuses the standby's role, the standby therefore stands by forever without printing anything. Its probes (live 200, ready 503 with `standby: true`) look exactly like a healthy standby's, and it can never take over. Before round 4 the same state exited 1 with the refusal printed.
- What reaches it: a rotated password or `NOLOGIN` on the serving role, a revoked `pg_hba` entry, or a database that keeps refusing new connections. The leader keeps its existing session, so nothing shows until a failover fails.
- Possible fix: log the take's connection error, once per change of error, as a warning naming the lease. Optionally, put the reason in `/__spindle/ready`.

**5. Attacked and could not break**
- F1: repeated drops of the standby's own connection; it stands by on, then takes over.
- F3: with backoff, a 0.7 s row lock on a 1 s lease still keeps the leader. By my reading of the timing, the lock a renewal survives shrinks from about 0.95 to about 0.92 of the lease, still above the "four fifths" the docs claim. Renewals land 10 per 2 s on a healthy 1 s lease (round 3).
- 59403241: the paused-leader egress case is green 4 of 4 alone.
  - The re-check after the read closes the pause path.
  - A leader whose lease session drops while it runs can still post events the new leader committed, until its next renewal. Those posts carry the same `(component, log, position)`, which your decision admits. That window should rarely open, since the old leader usually exits before the new leader commits, so I did not write a case for it.

**6. Paths written outside the worktree** (all under `<local path>`)
- `r3-amend.log`
- `r3-amend-clippy.log`
- `commit4.txt`
- `r4-run1.log`
- `r4-run2.log`
- `r4-run3.log`
- `r4-lane.log`
- `r4-clippy.log`
- `commit5.txt`

The first three are from the earlier amendment; the remaining files from rounds 1 to 3 are still there too.

The role `<schema>_r` created by the red case stays in the throwaway lane cluster, which is removed after each run.

**7. Findings block**
```findings
- file: crates/spindle-kernel/src/postgres.rs
  line: 533
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a standby whose lease connection the database refuses from now on reads the lease as held without a word, so it stands by forever looking like a healthy standby though it can never take over
```

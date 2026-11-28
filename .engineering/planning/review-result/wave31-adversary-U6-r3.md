---
format: aep.planning-md/3
id: review-result:wave31-adversary-U6-r3
kind: review-result
status: active
title: Adversary, round 3, on wave 31 U6
relations:
- reviews: story:several-writers-one-postgres
revision: 1
---
unit: story:several-writers-one-postgres, round 3. Findings cover head 6c611de9, which is 37f4226a plus my test commit.
verdict: CONFIRMED
cases: executed 127→132, red 2
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 15 new paths in the U6 adv-scratch directory (part 6)
needs-coordinator: none

Change 1 holds for the leader: it exits 1, and no write is lost or applied twice. Two things fail. The webhook gets the last 4 or 5 events twice across that failover. A standby whose own lease connection drops exits 1 instead of standing by. Changes 2 and 3 hold. The lane step's growth comes from node load, not from a test.

**1. Diff stat (37f4226a..HEAD).** One test file, no other path.
```
 .../tests/spindle-cli/adversary_w31_u6.rs          | 285 +++++++++++++++++++++
```

**2. Cases added** (module `adversary_w31_u6::postgres`, run alone in the lane before the suite)
- **`a_dropped_standby_session_keeps_it_standing_by`: red, every run.** The test terminates the standby's own lease connection, expecting it to keep standing by and take over once the leader is killed. Output:
  ```
  the standby exited once its lease connection was dropped: b10x-spindle: standing by: another process holds the lease of board, so this run serves nothing until it takes the leases of board, ledger
  b10x-spindle: store: the lease of board was not read or written: connection closed
  ```
- **`a_dropped_leader_session_fails_over_with_each_write_once`: red, 6 of 7 runs.**
  - Setup: commands flow to the leader, with an egress webhook attached, and the test terminates the leader's lease connection.
  - What holds: the leader exits 1 and logs that it writes nothing more. The standby leads. Every command answered 201 is held once, and none answered 503 landed.
  - What fails: the egress duplicates. Output:
  ```
  b10x-spindle: the egress cursor of board for `hook` was not saved at 94, so its events up to there are posted again after a restart: store: fenced: the lease of board was taken by another process (token 2, this process took token 1), ...
  posted more than once across the failover: ["f25", "f26", "f27", "f28", "f29", "f25", "f26", "f27", "f28", "f29"] of 35 posts
  ```
  - Earlier versions of my case had bugs I fixed: the stderr check was too narrow and ran before the line was read. The red output above is from the fixed case.
- **`renewals_on_the_shortest_lease_come_every_fifth_of_it`: green.** The lease row's `until` moved 10 times in 2 s on a 1 s lease. No renewal storm on a healthy leader.
- **`a_shortest_lease_held_up_by_a_row_lock_keeps_leading_and_posting`: green.** On a 1 s lease the row is locked for 0.7 s. A command sent during the lock gets 201, its event is posted 5 ms after the answer, the leader stays, and the standby does not take over.
- **`a_healthy_leader_posts_promptly_on_a_short_and_a_long_lease`: green.** Across 4 runs, no egress read was refused. The slowest post after its answer was 101 to 107 ms in most runs, 507 ms once alone, and 1.27 s once under lane load.
  - The 1.27 s case had no refusal line, so it is lane load: the feed waits for other tests' transactions.
  - I changed this case's bound from 1 s to "no `cannot read the store` line, and under 5 s".

**3. Suite, run after the cases existed**
- `slot --timeout 30m sh ./.pglane.run.sh task check:postgres`: `Summary [41.190s] 132 tests run: 130 passed, 2 failed, 1463 skipped`. Within reports `41.712 s of 120 s`; the harness exited 201. The 2 failures are the red cases above. "Before" is 127: the same run with my 5 round-3 cases left out.
- `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings`: exit 0.
- `cargo fmt -p spindle-cli --check`: exit 0.

**Why the lane step grew:** the same tree measured 26.7 s and then 41.2 s ten minutes apart. Round 2 measured 21.2 s.
- The slowest tests are the same each time: kernel scan adversaries (`adv2_a_scan_answers_over_a_million_events` 16.2 to 18.9 s, `adv3_drawn_command_sequences_scan_as_sqlite_does` 12.2 to 15.7 s), `adversary_state_move::a_refused_move_leaves_every_table…` (13.8 to 14.3 s), and `postgres::a_dead_letter_survives_on_postgres` (8.9 to 14.7 s).
- No U6 test is in the top 15. Mine run 2.6 to 4.9 s, and `replicas::a_standby_posts_no_webhook…` ran 9.4 s.
- So the 43.2 s comes from build-node load on those same tests, not from a test that grew.

**4. Findings**
1. **`crates/spindle-cli/src/database.rs:332`: concurrency, warning, CONFIRMED, introduced.**
   - What happens: `Leases::take` passes on a closed-connection error from `self.lease.take(component)?`, so a standby exits 1 after one dropped lease connection. Round 2 treated only lock and statement timeouts as "held".
   - What reaches it: the drops change 1 now treats as normal (a network blip, a server idle-session timeout, a proxy timeout, a database restart). The supervisor restarts the standby; nothing is lost, but there is no standby meanwhile.
   - Possible fix: in a standby poll, read a closed connection as "held" and reconnect on the next poll.
2. **`crates/spindle-cli/src/egress.rs:354` (the cursor save the fence refuses): concurrency, note, CONFIRMED, introduced.**
   - What happens: the old leader posts its last events, then saves the cursor one loop later. Once the standby holds the lease, that save is refused, so the new leader posts the last 4 or 5 events again.
   - What reaches it: since change 1, any drop of the leader's lease connection, besides a kill.
   - The egress contract is at-least-once, and its docs name only "one answered just before a SIGKILL" as posted again. A failover on a clean exit 1 duplicates several. The coordinator's "egress does not double-post" does not hold.
   - Possible fix: either document duplicates on failover in the egress contract, or save the cursor before posting the next page.

Not raised as a finding: a failed renewal is retried after 50 ms without waiting. Against a database that refuses connections fast, the leader reconnects up to about 20 times a second until a whole lease has passed. I did not measure it, because stopping the shared lane database would break other tests.

**5. Attacked and could not break**
- Change 1, leader side: it exits 1 and logs it, the standby takes over, and writes are neither lost nor doubled (7 runs).
- Change 2: about one renewal per fifth of the lease on a healthy 1 s lease. A 0.7 s row lock on a 1 s lease keeps the leader, as the "four fifths" claim says.
- Change 3: a healthy leader's egress is never held back on a 1 s or 10 s lease, and it resumes after a held-up renewal (posted 5 ms after the answer).
- The round-1 and round-2 cases stay green.

**6. Paths written outside the worktree** (all under `<local path>`)
- `r3.diff`
- `r3-run1.log`
- `r3-run2.log`
- `r3-run3.log`
- `r3-run4.log`
- `r3-lane.log`
- `r3-lane2.log`
- `r3-clippy.log`
- `commit3.txt`

The files from rounds 1 and 2 are still there: `run1.log` to `run4.log`, `lane.log`, `clippy.log`, `commit.txt`, `r2-run1.log` to `r2-run3.log`, `r2-lane.log`, `r2-clippy.log`, `commit2.txt`.

**7. Findings block**
```findings
- file: crates/spindle-cli/src/database.rs
  line: 332
  category: concurrency
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a standby whose own lease connection is dropped exits 1 on the closed-connection error of its next poll instead of standing by and reconnecting
- file: crates/spindle-cli/src/egress.rs
  line: 354
  category: concurrency
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a failover after the leader's lease connection drops posts the old leader's last four or five events again, because their cursor saves are refused by the fence once the standby took the lease, beyond the SIGKILL case the egress contract names
```

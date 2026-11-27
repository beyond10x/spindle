---
format: aep.planning-md/3
id: review-result:wave31-adversary-U6-r8
kind: review-result
status: active
title: Adversary, round 8, on wave 31 U6
relations:
- reviews: story:several-writers-one-postgres
revision: 1
---
Round 7 is red: 2 of the 4 new cases fail. A session outside spindle that holds the lease row now stops all writes with no time limit, and each ending writes an "apparent wraparound" ERROR to the server log. All six round-6 cases now pass, so F1 to F5 hold. The new cases are committed as `f915fe75` on top of `7d2a1e27`.

```
unit: U6 wave 31, story:several-writers-one-postgres, adversary round 7, head 7d2a1e27 -> f915fe75
verdict: red
cases executed: 147 -> 151 (lane `task check:postgres`)
red: 2
origins: introduced 2, pre-existing 0, undecided 1
wrote outside worktree: scratch only; needs coordinator: yes (decide G1)
```

**Diff stat**
`crates/spindle-cli/tests/spindle-cli/adversary_w31_u6.rs`: +193 lines, 1 file. No other file touched; `.pglane.*` was removed before the commit.

**Cases** (2 s lease, pool of 2, all in the PostgreSQL lane)
1. `a_foreign_lock_on_the_lease_row_stops_writes_no_longer_than_the_store_deadline` (line 2074): **red**.
   - Set-up: a session outside spindle holds board's lease row `FOR UPDATE` and keeps it.
   - Result after 14 s: `None`. The foreign session is alive, the standby printed only its standing-by line, and the leader is running and printed nothing.
   - `t2` answered `501 {"committed":false,"refused":"store: event store deadline exceeded during database statement or lock"}`.
2. `a_paused_leader_behind_a_foreign_row_lock_loses_its_lease` (2130): green.
   - With the same foreign lock, the leader is paused. The standby ends the foreign session and takes over within 5 s; the old leader exits 1.
3. `a_write_behind_a_foreign_table_lock_fails_in_time_without_losing_the_lease` (2168): green.
   - Board's event table is held `ACCESS EXCLUSIVE` for 12 s, past the store's 10 s deadline.
   - The command is answered in under 11 s, neither 201 nor 503, and is not held. The standby takes nothing, and the leader writes `t3` 201 afterwards.
4. `ending_a_plain_holder_logs_no_wraparound_error` (2227): **red**.
   - Result: `the server logged 1 line(s) of 'apparent wraparound' while the standby ended one session holding a plain transaction's row lock`.
   - It reads the log through `pg_read_file(pg_current_logfile())`. Where the server keeps no log file of its own (likely CI's image) it prints that the log was not read and passes.

**Suite run**
- Round-6 cases plus cases 1 to 3, run alone: 8 of 9 passed; only case 1 failed.
- Five ending cases plus the implementor's `a_standby_ends_only_...`, run with a scratch harness that prints the server log:
  - 4× `ERROR: MultiXactId N has not been created yet -- apparent wraparound`, one per ending;
  - 19× `ERROR: canceling statement due to statement timeout`, from takes and renewals waiting on the row;
  - 6× `FATAL: terminating connection due to administrator command`, the sessions ended.
- Full lane: 30.5 s, 151 tests, 148 passed, 3 failed, no `too many clients` with `max_connections=300`:
  - my 2 reds;
  - `postgres::an_egress_cursor_survives_on_postgres` (`the webhook accepted 0 of 1 bodies within 10s`). This is not my case: the PostgreSQL feed lag under lane load, the same lag that made me widen my own waits to 30 s.
- Slowest: my round-3 `a_healthy_leader_posts_promptly...` at 30.5 s (SLOW). New cases take 2.2 to 15.5 s.
- Clippy (`-D warnings`) and `cargo fmt --check` are clean for spindle-cli.

**Findings**
- **G1 (medium, introduced): a foreign holder of the lease row now causes an outage with no time limit.**
  - Where: `crates/spindle-cli/src/database.rs:510,525` and `crates/spindle-kernel/src/postgres.rs:836` (`PAST`).
  - The rule "renewals that waited out the row's lock do not count" does not tell the leader's own held-up write apart from any other holder.
  - Behind a foreign holder, every renewal waits, so the leader never gives up. Its lease connection runs a statement at least every third of the lease, so `PAST` and `TAKE` read it as live, and the standby neither ends the holder nor takes the lease. Every write fails.
  - The standby prints nothing beyond its standing-by line, and the leader logged nothing in the 14 s window. Before round 7, the leader gave up after one lease and the standby ended the holder.
  - The docs (`environment-and-admission.md`) scope the rule to "the leader's own write".
  - Possible fix: count a wait as the leader's own only when one of its pool sessions holds the row. For example, the renewal checks that the row's `xmax` lock (or a member's) belongs to a session with the same `client_addr` and port set, or the store records its write's backend pid. Otherwise let the give-up count, or let the standby end holders that are not the live holder's own.
- **G2 (low, introduced): each ending writes a false "apparent wraparound" ERROR.**
  - Where: `postgres.rs:548,846` (`MEMBERS`).
  - Every ending on a plain `xmax` runs `pg_get_multixact_members` on a transaction id. The server then logs `ERROR: MultiXactId N has not been created yet -- apparent wraparound`, which reads to a DBA as a wraparound emergency.
  - In a database where the multixact counter has passed that number, the lookup instead returns the members of an unrelated multixact, which are matched as holders. They are only ended if they also hold a row lock on the lease table.
  - Possible fix: tell a multixact `xmax` apart before the lookup, for example `pgrowlocks` where installed. Or look up only when `xmax` matches no `transactionid` lock (a plain id always matches one while it is held), which skips the error in the common case.
- **G3 (low, undecided): `an_egress_cursor_survives_on_postgres` failed once.**
  - Its 10 s webhook wait is shorter than the feed lag seen under lane load (up to about 20 s).

**What could not be broken**
- A hung leader cannot keep its lease through the half-lease rule. When paused, its lease connection's statement is cut at a third of the lease, it goes idle, and the standby takes over (case 2).
- A leader blocked behind a foreign table lock: its write fails within the store's deadline and the leader keeps leading (case 3).
- The 503 mapping:
  - A store failure while the lease is held stays 501, not 503 (case 3).
  - The paused leader's in-flight command is now 503 (round-6 case 3 green).
  - A committed write is only turned into 503 if its reply says `committed: false` while the commit landed, an in-doubt commit at the moment the session ends. That could not be produced deterministically here.
- No spurious "cannot identify" warning appeared in any run. Two remaining ways to get one, neither tested:
  - a holder that commits between a take's timeout and the ending;
  - a foreign table lock without a row lock (`CREATE INDEX`'s SHARE lock), where the warning's "row is still held" text would be inaccurate.
- Multixact holders (round-6 case 4) are now ended. A bystander holding only a read lock on the lease table is never ended.

**Paths written outside the worktree**
All in `<local path>`: `r7-alone.log`, `r7-serverlog.log`, `r7-wrap.log`, `r7-lane.log`, `pg-lane-log.sh` (a harness copy that prints server ERROR lines, not committed), `commit8.txt`.

```yaml
findings:
  - id: G1
    severity: medium
    origin: introduced
    where: crates/spindle-cli/src/database.rs:525
    case: adversary_w31_u6::postgres::a_foreign_lock_on_the_lease_row_stops_writes_no_longer_than_the_store_deadline
    summary: a session outside spindle holding the lease row keeps the leader live and stops every write with no time limit; standby silent, nothing ended
  - id: G2
    severity: low
    origin: introduced
    where: crates/spindle-kernel/src/postgres.rs:846
    case: adversary_w31_u6::postgres::ending_a_plain_holder_logs_no_wraparound_error
    summary: the member lookup on a plain xmax logs "MultiXactId N has not been created yet -- apparent wraparound" on the server at every ending
  - id: G3
    severity: low
    origin: undecided
    where: crates/spindle-cli/tests/spindle-cli/egress.rs:103
    case: postgres::an_egress_cursor_survives_on_postgres
    summary: its 10 s webhook wait is shorter than the lane's PostgreSQL feed lag under load; failed once in the full lane
```

---
format: aep.planning-md/3
id: review-result:wave31-adversary-U6-r9
kind: review-result
status: active
title: Adversary, round 9, on wave 31 U6
relations:
- reviews: story:several-writers-one-postgres
revision: 1
---
Round 8 is green: the full lane passes 144 of 144. Nothing of the ending is left in the code or docs, the documented bound holds (takeover 10.06 s after a pause inside a write, against 13.33 s allowed), and the 503 mapping holds both ways. The test changes are committed as `c368dbf9` on top of `e3aaec00`; the only new finding is a CHANGELOG wording gap.

```
unit: U6 wave 31, story:several-writers-one-postgres, adversary round 8, head e3aaec00 -> c368dbf9
verdict: green
cases executed: 144 -> 144 (lane `task check:postgres`; no new cases, 3 amended)
red: 0
origins: introduced 1, pre-existing 0, undecided 0
wrote outside worktree: scratch only; needs coordinator: no
```

**Diff stat**
`crates/spindle-cli/tests/spindle-cli/adversary_w31_u6.rs`: +92 / -361 lines, 1 file. `.pglane.*` was removed before the commit.

**Cases**
- **Deleted**, as you listed:
  - `a_live_leaders_write_held_up_past_a_short_lease_is_not_ended`
  - `a_leader_whose_write_a_standby_ended_answers_consistently_and_writes_on`
  - `a_lease_row_two_sessions_share_past_its_end_is_not_waited_on_in_silence`
  - `a_standby_ends_no_session_holding_another_components_running_lease`
  - `ending_a_plain_holder_logs_no_wraparound_error`
  - their helpers `held_by_the_test` and `server_log_lines`
- **Amended:**
  - `a_leader_paused_inside_a_write_answers_that_command_503_and_lands_nothing` (line 1716): green.
    - Bound: one lease + 10 s + one ask = 13.33 s from the pause; measured `took over 10.056981065s after the pause`.
    - Commands now wait up to 30 s for their answer (new helper `fire_for`), so the paused command is answered after the pause rather than timing out.
    - Kept: the in-flight command is answered 503 and not held, every 201 is held, and the old leader exits 1.
    - Added: the standby prints no ending line.
  - `a_paused_leader_behind_a_foreign_row_lock_loses_its_lease` (1909): green.
    - Measured: `paused 5.006s; took while held false; foreign session alive true, committed Ok(()); took true 60ms after the row was let go`.
    - The old leader exits 1 once resumed.
  - `a_foreign_lock_on_the_lease_row_holds_up_the_takeover_and_the_leader_exits_1` (1855): green. This is round 7's red G1 case, renamed and moved to the documented contract.
    - Measured: `leader exited Some(1) after 2.29s; t2 answered 503 "the run lost the lease of its store to another process and exits"; took while held false; took true 60ms after the row was let go`.
    - The new leader writes `t3` 201, and `t2` is not held.
- **Kept as they were:**
  - `after_a_failover_the_standby_left_ends_no_session_of_the_new_leader` (1801): green. Its check that no process prints the ending line now guards the revert.
  - `a_write_behind_a_foreign_table_lock_fails_in_time_without_losing_the_lease` (1953): green.
    - Measured: `t2 answered 501 after 2.00s: {"committed":false,...deadline exceeded...}`. A failure while the lease is held is not turned into 503, and the leader keeps leading.

**Suite run**
- Five cases alone: 5 of 5 passed, in 18.0 s.
- Full lane: 33.3 s, 144 of 144 passed, no `too many clients` error.
- Slowest: my round-3 case `a_healthy_leader_posts_promptly...`, 33.3 s (marked SLOW); the amended paused case takes 17.9 s.
- Clippy (`-D warnings`) and `cargo fmt --check` are clean for spindle-cli.

**Findings**
- **H1 (low, introduced, docs only): the CHANGELOG bound is too short for leases over 30 s.**
  - `CHANGELOG.md` says the takeover comes "at most one lease plus 10 s after the pause". The environment page adds "and one ask of the standby", which is needed.
  - After the lease's end, the standby's next ask comes up to a third of the lease later. Once the lease is over 30 s, that third is more than the 10 s idle timeout, so the bound becomes one lease plus a third of it. At 60 s that is 80 s, against the CHANGELOG's 70 s.
  - Not tested: it needs a lease over 30 s, which does not fit the 60 s test budget.
  - Fix: add "and one ask" to the CHANGELOG line.

**What could not be broken**
- **Leftovers of the ending:** none.
  - Code and docs: no `Ending`, `end_holders`, `waited()`, multixact lookup or "ran out while" text remains. `database.rs` and `postgres.rs` match `f503be99` apart from the 10 s cut being removed. The one mention of `pg_terminate_backend` (`postgres.rs:657`) is in an older comment.
  - At run time: no process prints the ending line in the failover and paused cases.
- **The documented bound:** a pause inside a write was taken over at 10.06 s, within 13.33 s.
- **The foreign row lock contract:** no takeover while the row is held, the leader exits 1 after about one lease, and the takeover follows within 60 ms of the row being let go.
- **The 503 mapping:**
  - An uncommitted write after the lease is lost answers 503, for both the paused command and the command behind the foreign row lock.
  - A store failure while the lease is held stays 501.
  - A committed write (201) is never remapped. Only an in-doubt commit at the moment a session ends could still be misreported, and that cannot be produced deterministically.

**Paths written outside the worktree**
All in `<local path>`: `t8a.rs`, `t8b.rs`, `new8.rs`, `r8-alone.log`, `r8-lane.log`, `commit9.txt`.

```yaml
findings:
  - id: H1
    severity: low
    origin: introduced
    where: CHANGELOG.md:177
    case: none
    summary: the CHANGELOG's "one lease plus 10 s" takeover bound omits the standby's ask; for a lease over 30 s the takeover may come up to a third of the lease later than the 10 s
```

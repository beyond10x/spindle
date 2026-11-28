---
format: aep.planning-md/3
id: review-result:wave32-adversary-U3-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 32 U3
relations:
- reviews: story:leader-exits-promptly-on-sigterm
revision: 1
---
unit: story:leader-exits-promptly-on-sigterm (U3); the findings cover head 002917b37 (unit head 0244897ef plus my test commit)
verdict: CONFIRMED
cases: executed 147→150 in the PostgreSQL lane and 511→512 in the spindle-kernel test binary; red 1
origin: introduced 3 / pre-existing 1 / undecided 0
wrote-outside-worktree: 10 paths (part 6)
needs-coordinator: two finished probe worktrees are still registered, and removing them is yours

The bound fails on one path: a leader on PostgreSQL can still exit later than the bound `run --help` now promises. An events read that is in flight at SIGTERM holds the exit until the other session's transaction ends. That wait was already there on the base, but the new help text and changelog entry promise the bound for exactly this situation.

**1. Diff after my commit** (`git diff --stat 0244897ef..HEAD`): test files only.
```
 crates/spindle-cli/tests/spindle-cli/adversary_w32_u3.rs       | 548 +++
 crates/spindle-cli/tests/spindle-cli/main.rs                   |   1 +
 crates/spindle-kernel/tests/spindle-kernel/adversary_w32_u3.rs | 200 +++
 crates/spindle-kernel/tests/spindle-kernel/main.rs             |   1 +
```

**2. Cases added**, first run alone in one lane job (`adv-scratch/cases-alone.out`, 4 tests: 3 passed, 1 failed):
- **Red:** `adversary_w32_u3::postgres::a_leader_with_an_events_read_under_way_exits_within_the_bound_under_an_open_transaction` (6.9 s alone, 10.6 s in the lane).
  - Steps: the leader writes t1; another session opens a transaction; the leader writes t2; a client sends `GET /api/events`; then SIGTERM, with the transaction held until 1 s past the bound. There is no egress, so the bound is 5 s.
  - Output:
    ```
    the leader exited 6.034018088s after SIGTERM, past the 5s `run --help` names, held by an events read waiting for the feed (status ExitStatus(unix_wait_status(36608)))
    ```
  - In the full lane it exited 9.6 s after SIGTERM, because transactions from other tests also held the feed back. On the base commit it exits at 6.03 s, so the wait is pre-existing.
- **Green, kept:** `a_post_under_way_at_sigterm_is_saved_before_the_standby_takes_over` (5.9 s). The webhook answers 1.5 s late and SIGTERM arrives mid-post; the test asserts t1 is posted exactly once.
  - On a copy where the leases are released before the egress stops, it goes red: `t1 was posted once ... the egress cursor of board for hook was not saved at 4 ... fenced`.
  - The unit's own `a_leader_exits_within_the_bound_under_an_open_transaction` and `a_standby_posts_no_webhook_and_keeps_no_dead_letter` stay green on that same copy.
- **Green, kept:** `postgres::a_renewal_after_a_release_that_waited_out_its_lock_waits_a_third_of_the_lease_again` (2.6 s). It checks that a failed release puts the connection's normal timeouts back.
  - On a copy that skips putting them back, it goes red: `waited 1.000894548s and answered Err(... statement timeout (SQLSTATE 57014))`.
  - The unit's `a_release_under_a_long_lease_waits_at_most_one_second` stays green on that copy.
- **Green, kept:** `a_sqlite_read_with_its_stop_flag_down_answers_as_the_read_without_one` (0.01 s). On SQLite the read with the stop flag down returns the same pages as before, at every position and limit; with the flag up it returns nothing.

The two mutants ran only in a separate probe worktree at the unit's head. Nothing under attack was changed.

**3. Suite**, run after the cases existed (`adv-scratch/suite.sh`, log in `suite.out`):
- `slot --timeout 30m --tokens 4 task check:postgres`: exit 201. `Summary [34.936s] 150 tests run: 149 passed (1 slow), 1 failed, 1504 skipped`. The 1 failure is my red case.
- `cargo test --locked -p spindle-kernel --no-fail-fast`: exit 0. `512 passed; 0 failed; 61 ignored`.
- `cargo clippy --locked -p spindle-kernel -p spindle-cli --all-targets -- -D warnings`: exit 0, and still exit 0 after the formatting fix.
- `cargo fmt -p spindle-kernel -p spindle-cli --check`: exit 1 on my own files. I fixed the formatting by hand and it then gave exit 0.
- I did not run the full `cargo test -p spindle-cli`: my spindle-cli cases are all lane-only (`#[ignore]`), so it would run the same set as before.

**4. Findings**
- **Exit held by an events read** (`crates/spindle-ingress/src/listener.rs:650`, CONFIRMED, pre-existing).
  - What was measured: the red case above. In `Listener::close`, `server.join()` waits for the listener's runtime to be dropped, and that drop waits for any read handed to a blocking thread (:977). Such a read can sit in `LogStore::published` for up to 30 s (`PUBLISH_WAIT`).
  - What reaches it: the console is on by default on loopback. `GET /api/events` and `GET /api/history` are answered through that blocking read. The trigger is any client of those routes at SIGTERM, after the run wrote while another session held a transaction open.
  - Possible fix (not applied): end the server thread's runtime with `shutdown_timeout(grace)`, or pass the listener's stopping flag into the feed reads.
- **Help text promises a bound that does not hold** (`crates/spindle-cli/src/cli.rs:164`, plus `CHANGELOG.md` and the generated `cli.md`; NEEDS-CHANGE, introduced). The new text says the run exits within the bound "also while another session on the server holds a transaction open". The previous finding shows a reachable case where it does not.
- **Lease-before-egress order unguarded by the unit's tests** (`crates/spindle-cli/src/serve.rs:872`, note, introduced). The unit's tests do not catch releasing the leases before the egress stops, because the end-to-end case saves the cursor before SIGTERM. My kept post-under-way case now covers it.
- **Timeout restore unguarded by the unit's tests** (`crates/spindle-kernel/src/postgres.rs:665`, note, introduced). Restoring the timeouts after a release is not tested by the unit. My kept renewal case now covers it.

**5. Attacked and could not break**
- A stopped read never drops an event that must still be posted: it returns an empty page, and the cursor only moves after a post succeeds.
- The stop flag does not change SQLite behaviour.
- A post under way at SIGTERM is finished and its cursor saved before the leases are released.
- Timeouts are restored after a release that waited out a locked row.
- The `events_id` wait for a read slot cannot be reached: each egress lane has its own reading handle with one read at a time.
- The renewal-in-progress and release waits, and a standby taking over while a release is still waiting, all need another session to hold the lease row's lock. That does not happen under the story's conditions, so I did not test them.
- I did not attack the 300 ms margin in the unit's end-to-end test.

**6. Paths written outside the worktree**
- Scratch files in `<local path>`: `cases-alone.out`, `base-caseA.out`, `mutants.out`, `suite.sh`, `suite.out`, `commit-msg.txt`.
- Probe worktrees, finished and archived but still registered:
  - `<local path>` (the base commit plus my spindle-cli module)
  - `<local path>` (the unit's head plus the two mutants)
- Their archives:
  - `<local path>`
  - `<local path>`

**7. Findings block**
```findings
- file: crates/spindle-ingress/src/listener.rs
  line: 650
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: a GET /api/events read in flight at SIGTERM waits in LogStore::published behind another session's open transaction, and Listener::close joins the server thread whose runtime drop waits for that blocking read, so the leader exits 6.0 s (9.6 s in the lane) after SIGTERM against the 5 s bound
- file: crates/spindle-cli/src/cli.rs
  line: 164
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: run --help and the CHANGELOG now promise the exit bound also while another session holds a transaction open, which a reachable events or history read in flight breaks
- file: crates/spindle-cli/src/serve.rs
  line: 872
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: releasing the leases before the egress halts leaves the unit's end-to-end and dead-letter cases green, because the cursor is saved before SIGTERM; the kept adversary case catches it
- file: crates/spindle-kernel/src/postgres.rs
  line: 665
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: dropping the restore of the connection's timeouts after a release leaves the unit's release case green; the next renewal then fails at 1 s, which the kept adversary case catches
```

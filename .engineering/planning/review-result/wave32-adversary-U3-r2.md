---
format: aep.planning-md/3
id: review-result:wave32-adversary-U3-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 32 U3
relations:
- reviews: story:leader-exits-promptly-on-sigterm
revision: 1
---
unit: story:leader-exits-promptly-on-sigterm (U3) round 2; the findings cover head b7b443825 (fix e5f9b0f43 plus my test commit)
verdict: CONFIRMED
cases: executed 150→152 in the PostgreSQL lane, 185→186 in the spindle-ingress test binary; red 1
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 7 paths (part 6)
needs-coordinator: three finished probe worktrees are still registered, and removing them is yours

Round 2 broke on one point: a `GET /api/events` that was already under way when the signal came is answered `503`, even when it had already read every event. `run --help` still says "the request under way finishes". Before the fix the same read answered `200`. Exit with a promoted standby and the read-slot waits both held up.

My round-1 files are unchanged: `git diff 002917b37 e5f9b0f43 -- '*adversary_w32_u3*'` prints 0 lines.

**1. Diff** (`git diff --stat e5f9b0f43..HEAD`): test files only.
```
 crates/spindle-cli/tests/spindle-cli/adversary_w32_u3.rs         | 143 +++
 crates/spindle-ingress/tests/spindle-ingress/adversary_w32_u3.rs | 261 +++
 crates/spindle-ingress/tests/spindle-ingress/main.rs             |   1 +
```

**2. Cases added**, first run alone in one lane job (`adv-scratch/r2-cases-alone.out`, 3 tests: 2 passed, 1 failed):
- **Red, no database needed:** `spindle-ingress adversary_w32_u3::an_events_read_under_way_that_read_every_event_is_answered_with_them_after_the_signal` (1.6 s).
  - Setup: a memory store whose events read takes 800 ms and waits for no feed. It stands in for SQLite reading a long log.
  - Steps: a `GET /api/events` arrives, then the stopping flag is set after 200 ms, then the read returns with all 3 events.
  - Output:
    ```
    assertion `left == right` failed: the read under way when the run was signalled had all three events in hand after 800.737231ms and answered 503 {"refused":"the run is stopping"}
      left: (503, 0)
     right: (200, 3)
    ```
  - At 002917b37 (before the fix, in a probe worktree) the same case passes (`adv-scratch/r1-ingress-case.out`), so the fix introduced it.
- **Green, kept:** `adversary_w32_u3::postgres::a_standby_that_leads_exits_within_the_bound_with_an_events_read_under_way` (8.0 s).
  - Steps: the leader is killed and the standby takes the lease (the `lead` path). The new leader writes t2 behind another session's open transaction, an events read is in flight, then SIGTERM.
  - Result: `exited 70.5ms after SIGTERM`, and the read answered `503`.
- **Green, kept:** `adversary_w32_u3::postgres::a_view_read_waiting_for_a_slot_behind_an_events_read_does_not_hold_the_exit` (7.1 s).
  - Setup: `database_pool` 2, which leaves one read slot. An events read waits for the feed and holds that slot, and `GET /tasks` queues behind it.
  - Result: exit 70.5 ms after SIGTERM; the view answered `200` with both rows 67 ms after the signal; the events read answered `503`.

**3. Suite**, run after the cases existed (`adv-scratch/r2-suite.sh`, log in `r2-suite.out`):
- `slot --timeout 30m --tokens 4 task check:postgres`: exit 201. `152 tests run: 151 passed (1 slow), 1 failed`.
  - The failure was `adversary_postgres_scan::adv2_a_serving_role_scans_a_log_it_never_scanned`: its `CREATE ROLE … GRANT …` setup hit a `db error` after 0.1 s.
  - I don't think it is related to U3. Rerun once (`r2-lane-rerun.out`): exit 0, `152 tests run: 152 passed (1 slow)`.
- `cargo test --locked -p spindle-ingress --no-fail-fast`: exit 101. `185 passed; 1 failed`, and the 1 is my red case.
- `cargo clippy --locked -p spindle-ingress -p spindle-cli --all-targets -- -D warnings`: exit 0.
- `cargo fmt … --check`: exit 1 on one line of mine. I reformatted it and it then gave exit 0.

**4. Findings**
- **A complete read under way is answered 503** (`crates/spindle-host/src/host/reader.rs:89`, the check after the read, and `crates/spindle-cli/src/cli.rs:161`; NEEDS-CHANGE, introduced).
  - What was measured: the red case above. `Reader::events_after` reads, then refuses because the flag is now set, whatever it read. This happens on every store, SQLite and memory included, not only PostgreSQL.
  - What reaches it: any events or history read that is under way when SIGINT or SIGTERM arrives, with the console on (the default on loopback).
  - Where it is documented: only the CHANGELOG, the `Reader::stopped_by` doc and the `Listener::stopping` doc mention it. `run --help` and `website/docs/reference/cli.md` still say "the request under way finishes".
  - Possible fixes (not applied): say in `run --help` that an events or history read under way is answered `503`; or refuse only a read that was actually given up, with the kernel returning a distinct "given up" result (as the head read already does) instead of an empty page plus a check of the flag.

**5. Attacked and could not break**
- **Standby that leads later:** `lead` wraps the reader with the listener's own stopping flag. Nothing resets that flag, and it is set only by the signal or at teardown, so a promoted standby carries no stale stop. The kept case passes.
- **Read-slot waits:** a view read queued behind a feed-waiting events read does not hold the exit; the events read releases its slot on the stop. `events_id`, scans and counts do not wait for the feed. I found no other read through the reader that does.
- **The 503 path itself:** an empty page from a stopped read is never answered as complete, because the flag is checked after it and is never reset.
- **SQLite:** the round-1 case (same pages with the flag down) still passes. The only SQLite change is the 503 in finding 4.

**6. Paths written outside the worktree**
- New scratch files in `<local path>`: `r2-cases-alone.out`, `r1-ingress-case.out`, `r2-suite.sh`, `r2-suite.out`, `r2-lane-rerun.out`, `commit-msg-r2.txt`.
- New probe worktree, finished and archived: `<local path>`, with its archive under `<local path>`.
- The two probe worktrees from round 1 are still registered as well.

**7. Findings block**
```findings
- file: crates/spindle-host/src/host/reader.rs
  line: 89
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: an events or history read under way at the signal that read every event is answered 503 on every store, SQLite included, while run --help still says the request under way finishes; it answered 200 before e5f9b0f43
```

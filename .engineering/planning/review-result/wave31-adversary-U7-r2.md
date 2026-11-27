---
format: aep.planning-md/3
id: review-result:wave31-adversary-U7-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 31 U7
relations:
- reviews: story:event-egress-webhook
revision: 1
---
unit: story:event-egress-webhook (wave 31 U7), round 2. Findings cover the correction 76392d94; my tests are commit 6137685b on top of it
verdict: CONFIRMED
cases: executed 657→665, red 1
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 9 new paths this round (part 6), plus the 10 from round 1
needs-coordinator: whether a reset matters to the body. After a reset on SQLite, `position` repeats an earlier event's value, so the body needs either an id of the log or a stated limit.

**Summary.** All three fixes hold, and my 3 red cases from round 1 are now green.
- **F1 (status alone):** holds for a chunked body that never ends, a 100 Continue before the status, and a webhook that answers before it reads the request body. The external-cause reply limit is still covered by the existing `adversary2_http_cause` cases, which are green.
- **F2 (each event at its own position):** holds across writes and restarts. The cursor stays the whole write's position: a kill between the two events of one write re-posts both, in order, at the positions they had. PostgreSQL gives strictly rising positions too, run on a throwaway cluster.
- **F3 (stop before each post):** holds for SIGTERM with `--drain 0` and with the default drain, and for a reset during an outage.

One new red: **positions repeat after a reset.** A reset on SQLite recreates the file, so the first event after it is posted at the same component and position as an earlier, different event. A consumer that de-duplicates by component and position would drop it. This was already true at ad7f3739, and the correction does not change it. On PostgreSQL a reset leaves the log in place and positions keep rising.

### 1. `git --no-pager diff --stat 76392d94..HEAD`
```
 .../tests/spindle-cli/adversary_w31_u7.rs          | 425 ++++++++++++++++++++-
 1 file changed, 422 insertions(+), 3 deletions(-)
```
Test file only. The 3 deleted lines are one match arm of the stub's `Reply::status` helper, rewritten to take the new reply variants. No test case was removed or weakened.

### 2. Cases added
All are in `<local path>`. The module run alone against 76392d94 gave `21 passed; 1 failed; 1 ignored; finished in 12.59s`. The 14 cases from round 1 are all green.

Red now:
- `an_event_after_a_reset_is_not_posted_at_an_earlier_event_s_position`:
  ```
  assertion `left != right` failed: t1 and t2 are posted as one component and position: [... "task_id": "t1" ... "position": Number(2) ..., ... "task_id": "t2" ... "position": Number(2) ...]
    left: (String("board"), 2)
   right: (String("board"), 2)
  ```

Green now, kept:
- `a_200_whose_chunked_body_never_ends_is_posted_once`
- `a_100_continue_before_the_status_is_read_past`
- `a_webhook_answering_before_it_reads_the_body_is_posted_once`
- `positions_rise_strictly_across_writes_and_a_restart`
- `a_kill_between_the_two_events_of_one_write_posts_both_again_in_order`
- `a_run_with_its_default_drain_stops_its_egress_after_the_post_under_way` (exit under 4 s)
- `a_reset_during_an_outage_answers_at_once_and_posts_what_follows` (the reset answers within 2 s)
- `postgres::the_events_of_one_write_rise_strictly_on_postgres`: ignored in the normal suite, so the PostgreSQL lane picks it up. Run through slot on the build node with a copy of the PostgreSQL harness:
  ```
  pg-lane: PostgreSQL 17.12 on 127.0.0.1:35485
  test adversary_w31_u7::postgres::the_events_of_one_write_rise_strictly_on_postgres ... ok
  test postgres::an_egress_cursor_survives_on_postgres ... ok
  test result: ok. 2 passed; 0 failed; ... finished in 1.05s
  ```
  The host itself has no `initdb`, so the harness has to run on the node.

No new test runs over 5 s; the longest is still the round-1 pause test, at about 11.5 s.

### 3. Suite run (after my cases existed)
- `cargo test --locked -p spindle-cli --no-fail-fast`: exit 101, 45 s, `661 passed; 4 failed; 35 ignored`. Red: my reset case and the 3 known UI cases.
- `cargo fmt -p spindle-cli --check`: exit 0. `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings`: exit 0.

### 4. Findings (cover 76392d94)
- **Positions repeat after a SQLite reset.** `crates/spindle-cli/src/egress.rs` builds the body from `placed`, the event's position in the feed. After `POST /api/reset` the SQLite file is new and positions start again from 1. The body carries nothing that tells the two logs apart.
  - What reaches it: `POST /api/reset` on a served SQLite run. It is on by default; the unit's own reset test uses it without a flag.
  - Fix, either one: add the log's id (`events_id`, which changes when the file is recreated) to the body, or state in the format that position identifies an event only within one log.
- **Note, no case written:** `GET /api/events` still reports the write's position, because the host's reading wrapper (`crates/spindle-host/src/host/rows.rs:113`) uses the default `events_placed_after`. For an event that is not the last of its write, the console and the webhook now show different positions. Nothing in the story ties the two together.

### 5. Attacked and not broken
- F1: an endless chunked 200, 100 Continue, an answer sent before the body was read.
- F2: positions rise across writes and restarts; a kill inside one write re-posts both events in order; PostgreSQL rises strictly.
- F3: SIGTERM with `--drain 0` (the round-1 case, now green) and with the default drain; a reset during an outage.

### 6. Paths written outside the worktree this round
In `<local path>`:
- `round1-module.rs.bak`, `round2-cases.rs`, `pg-lane.sh` (a copy of the harness), `commit-msg-r2.txt`
- logs: `r2-cases-alone.log`, `r2-pg.log`, `r2-fmt.log`, `r2-suite-after.log`, `r2-clippy.log`

The PostgreSQL cluster lived in the synced tree on the build node and the harness's cleanup trap removed it on exit. The round-1 mutant worktree is unchanged: finished, kept by gc until ad7f3739 reaches a remote.

### 7.
```findings
- file: crates/spindle-cli/src/egress.rs
  category: boundary
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: after a POST /api/reset on SQLite the first event is posted at the same component and position as an earlier different event, so a consumer de-duplicating by component and position drops it
- file: crates/spindle-host/src/host/rows.rs
  line: 113
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: GET /api/events still reports the write's position through the default events_placed_after while the webhook body reports each event's own position, so the two disagree for every event but the last of a write
```

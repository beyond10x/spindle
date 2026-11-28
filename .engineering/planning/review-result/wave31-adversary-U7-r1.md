---
format: aep.planning-md/3
id: review-result:wave31-adversary-U7-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 31 U7
relations:
- reviews: story:event-egress-webhook
revision: 1
---
unit: story:event-egress-webhook (wave 31 U7). Findings cover implementor head ad7f3739; my tests are commit 7cabc4f1 on unit/w31-egress
verdict: CONFIRMED
cases: executed 643→657, red 3
origin: introduced 4 / pre-existing 1 / undecided 0
wrote-outside-worktree: 10 paths (part 6)
needs-coordinator: what `position` means. The acceptance asks for "strictly rising `position`", but every event of one write carries the write's position. Either the code changes or the acceptance does; that is not my call.

**Summary.** Three red cases, all introduced by this unit:
- **A long 200 reply is treated as a failure.** If the webhook answers 200 with a body over 64 KiB, the run counts the post as failed. It re-posts the event forever and holds back every later event of that component.
- **Events of one write share one position.** When one write publishes two events, both bodies carry the same `position`. A consumer that de-duplicates by component and position would drop the second event.
- **Shutdown waits for the whole backlog.** After SIGTERM with `--drain 0`, the egress keeps posting the rest of its page to a slow webhook. The run exited 10.6 s after the signal instead of after the one post under way.

The unit's own test claims "the backoff is capped at 5 s" but passes with the cap at 60 s. I ran that mutant: the unit's test stays green and my kept test turns red. "At least once" and commit order held under every kill and outage I tried.

### 1. `git --no-pager diff --stat ad7f3739..HEAD`
```
 .../tests/fixtures/adversary-w31-u7-http.yaml      |  12 +
 .../fixtures/adversary-w31-u7/domains/pad.yaml     |  43 ++
 .../tests/fixtures/adversary-w31-u7/system.yaml    |  20 +
 .../tests/spindle-cli/adversary_w31_u7.rs          | 815 +++++++++++++++++++++
 crates/spindle-cli/tests/spindle-cli/main.rs       |   1 +
```
Only test files and fixtures changed. The fixture `adversary-w31-u7` is one component whose `OpenNote` publishes `NoteOpened` and `NoteCounted` in one write.

### 2. Cases added
All are in `<local path>`. The webhook is a stub on 127.0.0.1. First run of the module alone: `cargo test --locked -p spindle-cli --test spindle-cli -- adversary_w31_u7::` gave `11 passed; 3 failed; 677 filtered out; finished in 12.42s`.

Red now:
- `a_2xx_answer_with_a_long_body_is_not_posted_again`:
  `the webhook answered 200 to the first post, and was sent the event 5 times: [(Some(200), [("tasks.board.TaskOpened", "t1")]) ×5] left: 5 right: 1`
- `the_events_of_one_write_are_posted_with_strictly_rising_positions`:
  `positions rise strictly: [4, 4, 8, 8] for [("notes.pad.NoteOpened", "n1"), ("notes.pad.NoteCounted", "n1"), ("notes.pad.NoteOpened", "n2"), ("notes.pad.NoteCounted", "n2")]`
- `a_run_stops_its_egress_after_the_post_under_way`:
  `the run exited 10.566560133s after SIGTERM with --drain 0, and was sent 14 posts after the signal`

Green now, kept:
- `the_pause_between_two_posts_of_one_event_is_at_most_five_seconds` (about 11.5 s). Against the mutant `BACKOFF_CAP = 60 s` in a separate managed worktree:
  - `egress::an_event_committed_while_the_webhook_is_down_is_posted_when_it_returns ... ok`
  - mine: `every pause is capped at 5 s: [100.79ms, 201.05ms, 401.81ms, 803.24ms, 1.61s, 3.21s, 6.42s]`, FAILED
- `an_event_whose_post_was_under_way_at_a_kill_is_posted_after_the_restart`
- `a_kill_right_after_the_answer_loses_no_event_and_keeps_commit_order`
- `events_committed_during_an_outage_are_posted_once_each_in_commit_order`
- `events_pending_at_a_restart_are_posted_in_commit_order`
- `each_component_is_posted_on_a_lane_of_its_own` (board refused, ledger still delivered)
- `an_egress_naming_an_unpublished_event_exits_3_naming_it`
- `an_egress_to_a_put_call_is_refused_naming_post`
- `a_stdin_run_with_an_egress_exits_1`
- `a_redirect_is_not_followed_and_the_event_is_posted_again`
- `a_closed_connection_and_a_timeout_are_posted_again_until_a_2xx`

Tests over 5 s: the pause test (about 11.5 s) and `a_run_stops…` (about 10.6 s after SIGTERM while red).

### 3. Suite runs (after my cases existed)
- `cargo test --locked -p spindle-cli --no-fail-fast`, first run: exit 101, 36 s, `648 passed; 9 failed; 34 ignored`. Red: my 3, the 3 known UI cases, and 3 `seed::readiness::` cases.
- Same command, second run: exit 101, 36 s, `651 passed; 6 failed; 34 ignored`. Red: my 3 and the 3 known UI cases.
- The before count, with `-- --skip adversary_w31_u7::`: exit 101, 44 s, `640 passed; 3 failed; 34 ignored; 14 filtered out`.
- `cargo fmt -p spindle-cli --check`: exit 0, after formatting through `slot --fetch`. `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings`: exit 0.

### 4. Findings (cover ad7f3739)
- **Long 200 body re-posted forever.** `crates/spindle-cli/src/calls.rs:333` (`Call::post`) inherits `exchange`'s 64 KiB reply limit at `:396`. A 2xx with a longer or slow body becomes a failure: the event is re-posted every ≤5 s, the webhook accepts it each time, and the lane is stuck.
  - What reaches it: any webhook that answers 2xx with a large body.
  - Fix: decide on the status alone, and stop reading the body at the limit without failing.
- **Two events of one write, one position.** `crates/spindle-cli/src/egress.rs:457` posts the write's position (`crates/spindle-kernel/src/sqlite.rs:1382`, the group's maximum) for every event of the write.
  - What reaches it: any outcome with two `emits`, or an `affects` set publishing once per row.
  - Fix, either one: post each event's own feed position, or change the acceptance and add an index within the write. This is the needs-coordinator item.
- **The stop is ignored between posts.** `crates/spindle-cli/src/egress.rs:426` (the page loop) and `:463` (`post` sends before it checks the stop). After a stop, a lane posts up to 64 more events, each up to `timeout_ms` (at most 30 s). That blocks SIGTERM exit and also a `POST /api/reset` on the run's thread. The design comment at serve.rs:690 says "each after its post under way".
  - Fix: check `stop` before each post and leave the page.
- **The unit's backoff test cannot catch a missing cap.** `crates/spindle-cli/tests/spindle-cli/egress.rs:418` turns the webhook to 200 after 5 s, before the doubling pause ever reaches the cap, so the 5 s assertion cannot fail. My kept pause test now covers the cap.
- **Pre-existing seed flake, not this unit's.** `crates/spindle-cli/src/seed.rs:669-673` writes the seed's "ended" line before it publishes the state, at the base too (read with `git show 6306879b`). Under load, `seed::readiness` reads `running` after the end line: 3 red in one of the two suite runs. My added tests raise the load that exposes it.

### 5. Attacked and not broken
- A SIGKILL during a post or right after its answer: the event was re-posted, never lost.
- Order per store during an outage and across a restart.
- Two components on one call stay independent.
- 307 is not followed; a closed connection and a timeout are re-posted.
- Refusal text for an unpublished event (exit 3) and for a non-POST call.
- A stdin run with `[egress]` exits 1.
- The body fields match `spindle-egress/1`.
- A page boundary never splits a write (`groups_after` reads whole groups).
- A state move copies the cursor verbatim. PostgreSQL's `BIGSERIAL` positions are at or above SQLite's, so the worst case is a re-post, not a lost event. This is from reading the code; I did not run the PostgreSQL lane.

### 6. Paths written outside the worktree
- In `<local path>`: `cases-alone.log`, `mutant-cap.log`, `suite-after.log`, `suite-after-2.log`, `suite-without-mine.log`, `fmt.log`, `clippy.log`, `commit-msg.txt`, `net.toml`.
- The managed worktree `<local path>`: mutation reverted, `worktree finish --discard-cache` done. `worktree gc` keeps it until ad7f3739 is on a remote; removing it after that is yours.

### 7.
```findings
- file: crates/spindle-cli/src/calls.rs
  line: 333
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a 2xx answer whose body exceeds REPLY_LIMIT (or arrives slower than the timeout) counts as a failed post, so the event is re-posted without end and the component's later events never leave
- file: crates/spindle-cli/src/egress.rs
  line: 457
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: every event of one write is posted with the write's position, so positions of one component do not rise strictly and component plus position does not identify an event for de-duplication
- file: crates/spindle-cli/src/egress.rs
  line: 426
  category: concurrency
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a stopped lane keeps posting the rest of its page, so SIGTERM with --drain 0 exited 10.6 s late and a reset blocks the run's thread for up to 64 posts times timeout_ms
- file: crates/spindle-cli/tests/spindle-cli/egress.rs
  line: 418
  category: mutant
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the unit's test asserting the 5 s backoff cap stays green with BACKOFF_CAP at 60 s, because its webhook recovers before the doubling pause reaches the cap
- file: crates/spindle-cli/src/seed.rs
  line: 669
  category: concurrency
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: the seed's end line is written before its state is published, so seed::readiness tests read running after the end line under load (3 red in one of two suite runs)
```

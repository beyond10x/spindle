---
format: aep.planning-md/3
id: review-result:wave31-adversary-U7-r3
kind: review-result
status: active
title: Adversary, round 3, on wave 31 U7
relations:
- reviews: story:event-egress-webhook
revision: 1
---
unit: story:event-egress-webhook (wave 31 U7), round 3. Findings cover d2cdde70; my tests are commit 017a71f7 on top of it
verdict: nothing found
cases: executed 665→667, red 0
origin: introduced 0 / pre-existing 0 / undecided 0
wrote-outside-worktree: 9 new paths this round (part 6)
needs-coordinator: none

**Summary.** Decision D2 holds: every case of mine is green at d2cdde70, including on PostgreSQL. I amended `an_event_after_a_reset_is_not_posted_at_an_earlier_event_s_position` as you asked; it now compares (component, log, position) and is green. If `log` were missing, both posts would compare equal and the case would fail.

The gated reading handle in `rows.rs` now forwards `events_placed_after`, and the unit's test there checks it against the store's own handle. Nothing in production reads through that forward yet, though. `GET /api/events` still calls `events_after` and lists the write's position, so the round-2 note stands in narrower form.

### 1. `git --no-pager diff --stat d2cdde70..HEAD`
```
 .../tests/spindle-cli/adversary_w31_u7.rs          | 149 +++++++++++++++++++--
 1 file changed, 141 insertions(+), 8 deletions(-)
```
Test file only. The 8 deleted lines are the amended case's doc comment and its comparison, plus one `use` list of the `postgres` sub-module.

### 2. Cases (module run alone: `24 passed; 0 failed; 2 ignored; finished in 11.62s`)
- Amended, green: `an_event_after_a_reset_is_not_posted_at_an_earlier_event_s_position`.
- Added, green:
  - `the_log_is_named_in_each_body_and_kept_across_a_restart`: each body has exactly `component, event, fields, format, log, position`; `log` is a number other than 0, the same across a SIGTERM restart; positions keep rising.
  - `the_log_changes_on_a_sqlite_reset_and_is_kept_after_it`: the reset gives a new `log`, and a restart after it keeps that new `log`.
  - `postgres::the_log_is_kept_across_a_reset_and_a_restart_on_postgres`: ignored in the normal suite, so the PostgreSQL lane picks it up. `log` stays the same across a reset and a restart, and positions rise across both.
- PostgreSQL run, through slot on the build node with my copy of the harness:
  ```
  test adversary_w31_u7::postgres::the_events_of_one_write_rise_strictly_on_postgres ... ok
  test adversary_w31_u7::postgres::the_log_is_kept_across_a_reset_and_a_restart_on_postgres ... ok
  test postgres::an_egress_cursor_survives_on_postgres ... ok
  test result: ok. 3 passed; 0 failed; ... finished in 1.31s
  ```

### 3. Suite run, once
- `cargo test --locked -p spindle-cli --no-fail-fast`: exit 101, 37 s, `664 passed; 3 failed; 36 ignored`. The only reds are the 3 known UI cases.
- `cargo fmt -p spindle-cli --check`: exit 0. `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings`: exit 0.

### 4. Findings
- Still a note, narrower than round 2: `GET /api/events` lists each event at its write's position, while the webhook body uses the event's own position. For an event that is not the last of its write, the two disagree. No case written; nothing in the story ties them together.

### 5. Attacked and not broken
- G1:
  - `log` is present and the body has no other new key.
  - `log` is stable across a restart without a reset.
  - `log` changes on a SQLite reset and stays the same after it.
  - On PostgreSQL, `log` stays the same across a reset while positions keep rising.
- G2: the gated handle forwards `events_placed_after`, and the unit's own test would catch the missing forward.
- No other reading handle reaches the egress without that method: the memory store has none, and an egress on memory is refused.

### 6. Paths written outside the worktree this round
In `<local path>`:
- `round3-cases.rs`, `r3-top.rs`, `r3-pg.rs`, `commit-msg-r3.txt`
- logs: `r3-fmt.log`, `r3-cases-alone.log`, `r3-pg.log`, `r3-suite-after.log`, `r3-clippy.log`

The PostgreSQL cluster was on the build node and the harness removed it on exit.

### 7.
```findings
- file: crates/spindle-host/src/host/rows.rs
  line: 129
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the gated handle forwards events_placed_after but nothing in production reads through it, so GET /api/events still lists the write's position while the webhook body carries the event's own position
```

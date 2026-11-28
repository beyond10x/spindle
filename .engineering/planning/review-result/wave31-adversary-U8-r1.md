---
format: aep.planning-md/3
id: review-result:wave31-adversary-U8-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 31 U8
relations:
- reviews: story:dead-letters-kept-and-replayed
revision: 1
---
I broke the unit: two of my new cases fail, one in each crate I touched. The other seven pass and stay in as kept tests.

```
unit: story:dead-letters-kept-and-replayed, head b456ff30; findings cover b456ff30, my tests committed on top as 347fc467
verdict: CONFIRMED
cases: executed 716→723 (spindle-kernel 469→472, spindle-host 247→251; PostgreSQL lane +2), red 2
origin: introduced 4 / pre-existing 0 / undecided 0
wrote-outside-worktree: 10 paths under <local path> (listed in part 6)
needs-coordinator: none
```

## 1. What I changed (`git diff --stat b456ff30..347fc467`)

```
 crates/spindle-host/tests/spindle-host/adversary_w31_u8.rs   | 231 +++
 crates/spindle-host/tests/spindle-host/main.rs               |   2 +
 crates/spindle-kernel/tests/spindle-kernel/adversary_w31_u8.rs | 320 +++
 crates/spindle-kernel/tests/spindle-kernel/main.rs           |   2 +
```
Every path is a test file. No implementation file was touched.

## 2. Cases added

**`spindle-host` `adversary_w31_u8`** (each runs in under 0.05 s):
- **`adv_w31_u8_a_replay_naming_a_letter_a_reset_forgot_runs_nothing` — red.** Order o-forgotten is kept as letter 1. Then the run is reset, and order o-later becomes letter 1 too. Output of the first run, this case alone:
  ```
  panicked at crates/spindle-host/tests/spindle-host/adversary_w31_u8.rs:98:5:
  a dead letter kept after a reset reuses the forgotten id 1: [Undelivered { component: "shipping", id: 1, letter: DeadLetter { ... input: {"order_id": String("o-later")}, outcome: Some("busy"), ... attempts: 5 } }]
  test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 261 filtered out; finished in 0.04s
  ```
- `adv_w31_u8_a_replay_refused_with_an_escalating_outcome_is_not_escalated` — green. The replay does not escalate, publishes no ShipmentStuck event, and keeps a new letter with outcome `no-address`.
- `adv_w31_u8_a_letter_kept_before_a_restart_is_replayed_with_its_input` — green. After a restart the replay ships o-kept, and a third process sees the letter as taken.
- `adv_w31_u8_a_refused_tick_is_kept_without_an_event_and_replayed_as_a_tick` — green. Uses rally's `tick`: the letter has no event and keeps its input, the replay is attributed to `Cause::Tick`, and the refused replay is kept again without an event.

**`spindle-kernel` `adversary_w31_u8`** (each runs in under 0.2 s):
- `adv_w31_u8_scans_counts_and_a_reopened_fold_skip_dead_letters` — green. Covers the memory store, SQLite, a reopened SQLite file and the reading handle. This is claim 5, scans and folds.
- `adv_w31_u8_a_feed_page_of_dead_letters_between_two_events_reads_on` — green. 520 letters (more than one 500-event page) sit between two events, and every read still returns both events.
- `adv_w31_u8_the_version_of_a_take_is_no_dead_letter` — green.
- `adv_w31_u8_a_moved_file_keeps_its_dead_letters_on_postgres` (ignored, PostgreSQL lane) — green. `state move` copies the dead-letter stream with the same ids, applies the reset mark, and the moved letters can still be taken. This is claim 5, state move.
- **`adv_w31_u8_tables_holding_only_dead_letters_are_not_empty_on_postgres` (ignored, PostgreSQL lane) — red:**
  ```
  FAIL [0.106s] (3/5) spindle-kernel::spindle-kernel adversary_w31_u8::adv_w31_u8_tables_holding_only_dead_letters_are_not_empty_on_postgres
  panicked at crates/spindle-kernel/tests/spindle-kernel/adversary_w31_u8.rs:310:5:
  tables holding a dead letter are not empty
  Summary [0.169s] 5 tests run: 4 passed, 1 failed, 521 skipped
  ```

## 3. Suite run, after the cases existed (`adv-scratch/suite.sh`)

```
test spindle-kernel exit 0    test result: ok. 472 passed; 0 failed; 54 ignored; ... finished in 8.74s
test spindle-host exit 101    test result: FAILED. 250 passed; 1 failed; 14 ignored; ... finished in 10.84s
                              (the 1 failure is adv_w31_u8_a_replay_naming_a_letter_a_reset_forgot_runs_nothing)
clippy spindle-kernel exit 0
clippy spindle-host exit 0
```
`cargo fmt --check` first flagged layout in my two new files only. I ran `rustfmt --edition 2024` on those two files, and both crates' fmt check then exited 0.

The PostgreSQL lane ran as `slot --timeout 15m sh -c "<pg-lane.sh>" pg-lane cargo nextest run --locked --profile postgres -p spindle-kernel --test spindle-kernel --run-ignored all -E 'test(~adversary_w31_u8::)'`. Result: 5 run, 4 passed, 1 failed (shown above). The harness exited 100.

## 4. Findings

1. **A reset reuses dead-letter ids.**
   - **What was measured:** `adversary_w31_u8.rs:98` in `spindle-host`. On a run with `--state`, a reset replaces each SQLite file with an empty one (`emptied_file`, `crates/spindle-host/src/host.rs:2834`), and a memory store is replaced the same way. The dead-letter stream starts again at version 1, so the next letter takes the forgotten letter's id.
   - **Which promises it breaks:**
     - `spindle.observe.DeadLetter.id` says ids are "never reused while the store is kept".
     - `Store::keep_dead_letter` says each new id is greater than "a dead letter's it forgot included".
     - `ReplayAnswer` says the route answers `404` for a letter "a reset forgot".
   - **What reaches it:** a console user lists the letters, `POST /api/reset` runs, a new failure is kept, and a replay of the stale `{"component","id"}` then runs a different order's command.
   - **Fix:** carry the id counter across a reset, for example by seeding the new stream past the old store's last id. Or narrow the doc and the 404 text.
   - **Rating:** warning, CONFIRMED, introduced.

2. **`StateTarget::is_empty` ignores the dead-letter stream.**
   - **What was measured:** `crates/spindle-kernel/src/state_move.rs:268-292` checks only entity streams and `MARKS`. Tables that hold only `spindle:log/dead-letters` are reported as empty (`adversary_w31_u8.rs:310`).
   - **What reaches it:** a component on PostgreSQL whose bound commands were always refused holds no entity stream. `b10x-spindle state move` (`crates/spindle-cli/src/state.rs:98`) then passes its "nothing was moved" guard. `copy` fails with a conflict when it reaches the file's own dead-letter group, and the groups appended before that stay in the tables: a partial move.
   - **Fix:** add `DEAD_LETTERS` to the streams `is_empty` checks.
   - **Rating:** warning, CONFIRMED, introduced.

3. **The replay answer can describe another delivery** (from reading the code; I wrote no case for it).
   - **What I read:** `crates/spindle-ingress/src/console.rs:512-528` takes `report.ran.first()`, then `refused`, then `failed`, after `settle`. On a multi-process host, `settle_remote` (`crates/spindle-host/src/host/component.rs:794-822`) also delivers envelopes that peers sent in the meantime. If the replay itself was refused but a concurrent delivery of the same command ran, the answer says `"result": "ran"` with that other command's outcome.
   - **What reaches it:** a process started with `--component` that receives bound commands from a peer while a replay runs. I found no harness for this.
   - **Fix:** note which list `attempt` pushed the replay's entry into before calling `settle`.
   - **Rating:** note, CONFIRMED, introduced.

4. **A failed keep is silent.**
   - **What I read:** `Host::keep` (`crates/spindle-host/src/host.rs:2550-2553`) throws away the store error with `.ok()`. A letter that PostgreSQL refuses (for example U+0000 in the input) is printed as `dropped`, and nothing says why.
   - **Rating:** note, CONFIRMED, introduced.

## 5. Attacked and not broken
- Claim 1: a refused command prints no stderr line. The story's acceptance does not require one.
- Claim 2: a failed replay gets a new id, and the old id is not replayed again. Covered by the implementor's tests and mine.
- Claim 3: `forget_events` clears dead letters in memory and SQLite, and ids keep growing within one store. The id-reuse finding above comes only from the host replacing the store.
- Claim 4: the browser store keeps no letters by default, and the wasm `Shared` wrapper does not forward, which is consistent with that.
- Claim 5: scans, counts, a reopened SQLite file, the reading handle, `state move` copying with the same ids, and a feed page made only of dead letters. All green.
- Claim 6: the routes answer 404 with the console off, 400 for a malformed body, and the 404 text is right. The implementor's tests cover this.
- The list and replay answers match the `DeadLetter`, `DeadLetters` and `ReplayAnswer` schemas field for field. I checked by reading, not with a running case.
- A replay with an escalating outcome is not escalated.
- Ticks are kept without an event and replayed as a tick.
- I tried mutants against the implementor's tests (removing the `DEAD_LETTERS` skip in `groups_after`, removing the escalation check, passing the route to the replay, dropping the reset filter in `letters()`). Their tests catch each one.

## 6. Paths written outside the worktree
All under `<local path>`:
- `pg-lane.sh`
- `suite.sh`
- `commit.txt`
- `host-cases.log`
- `host-tick.log`
- `kernel-cases.log`
- `pg-cases.log`
- `suite-exit.txt`
- `suite-spindle-kernel.log`, `suite-spindle-host.log`
- `clippy-spindle-kernel.log`, `clippy-spindle-host.log`
- `fmt-spindle-kernel.log`, `fmt-spindle-host.log`

## 7. Findings block

```findings
- file: crates/spindle-host/src/host.rs
  line: 2834
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a reset replaces each store with an empty one, so the next dead letter reuses a forgotten id and a stale replay runs another order's command instead of answering 404
- file: crates/spindle-kernel/src/state_move.rs
  line: 268
  category: boundary
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: StateTarget::is_empty does not check the dead-letter stream, so tables holding only dead letters pass the state move guard and the copy then conflicts partway
- file: crates/spindle-ingress/src/console.rs
  line: 512
  category: concurrency
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the replay answer takes the first ran/refused/failed entry after settle, which on a multi-process host can be a concurrent peer delivery rather than the replay itself
- file: crates/spindle-host/src/host.rs
  line: 2552
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: Host::keep discards the store's error with .ok(), so a dead letter the store refuses is printed as dropped with no reason given
```

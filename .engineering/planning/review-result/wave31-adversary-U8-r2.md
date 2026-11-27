---
format: aep.planning-md/3
id: review-result:wave31-adversary-U8-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 31 U8
relations:
- reviews: story:dead-letters-kept-and-replayed
revision: 1
---
The correction holds: nothing I wrote against it fails. Both cases that were red in round 1 now pass, all 11 new round-2 cases pass, and I keep them all; what is left is three notes. Clippy could not run: the build node failed twice for a pool reason, not a lint.

```
unit: story:dead-letters-kept-and-replayed, correction head 6659a9ea; my round-2 tests committed on top as df70eb22
verdict: nothing found
cases: executed 894→901 (spindle-kernel 472→474, spindle-host 254→257, spindle-ingress 168→170; PostgreSQL lane 8→9), red 0
origin: introduced 0 / pre-existing 0 / undecided 0 (red cases); 3 notes below, all introduced
wrote-outside-worktree: 26 paths under <local path> (listed in part 6)
needs-coordinator: clippy for spindle-kernel, spindle-host and spindle-ingress did not run. It exited 3 twice with "slot shell: connect /run/user/1001/slot.sock: No such file or directory (os error 2) / slot: connection to daemon lost". Tests and fmt through the same pool were fine.
```

The `<before>` counts are the suite run after my cases existed, with my 7 new non-ignored round-2 cases and the 1 new PostgreSQL case taken out.

## 1. What I changed (`git diff --stat 6659a9ea..df70eb22`)
```
 crates/spindle-host/tests/spindle-host/adversary_w31_u8.rs       |  92 +++
 crates/spindle-ingress/tests/spindle-ingress/adversary_w31_u8.rs | 191 +++ (new)
 crates/spindle-ingress/tests/spindle-ingress/main.rs             |   1 +
 crates/spindle-kernel/tests/spindle-kernel/adversary_w31_u8.rs   | 121 +++
```
Every path is a test file.

## 2. Cases added (each passes now and runs in under 0.3 s)

**F1, ids continue across a reset**
- `spindle-host adv_w31_u8_r2_ids_grow_across_resets_in_a_row_and_a_restart`: four resets (two of them with no dead letter), a restart, a second restart. The ids keep growing, no forgotten id is replayed, and the waiting letter is replayed with `Attempted::Ran`.
- `spindle-kernel adv_w31_u8_r2_two_handles_and_a_reopened_file_agree_on_continued_ids`: two `SqliteStore` handles on one file. One continues the ids past 41 between the other's two keeps. Both handles list the same ids, a take works, and a reopened file keeps going past them. This exercises the forward read in `offset_before`.
- `spindle-kernel adv_w31_u8_r2_the_continue_event_is_no_event_row_or_count`: a file whose first event is the continue event. No event comes back from it, `events_id` is not 0, and scans and counts are unchanged before and after reopening.
- `spindle-kernel adv_w31_u8_r2_continued_ids_move_and_hold_their_tables_on_postgres` (ignored, PostgreSQL lane): a state move keeps the continued ids (all above 30), and new ids go past them.

**F2, `is_empty`**
- Same PostgreSQL case: tables holding only a continue event are not empty. My round-1 case `adv_w31_u8_tables_holding_only_dead_letters_are_not_empty_on_postgres` now passes too.

**F3, the replay's own run**
- `spindle-ingress adv_w31_u8_the_console_answers_a_replays_own_run_as_its_schema_says`: every list and answer is checked against the generated `spindle.observe.DeadLetters` and `spindle.console.ReplayAnswer` schemas. A replay refused with the escalating outcome `no-address` answers `refused`, outcome `no-address`, and the new letter's id; it publishes no escalation. A later replay answers `ran`, `shipped`, with no `dead_letter`; replaying it again gives 404. No list contains the order id.
- `spindle-ingress adv_w31_u8_a_replay_the_kernel_rejects_answers_failed_with_its_new_letter`: answers `failed`, with no outcome and a new `dead_letter` id, under the schema.

**Deviation 1, a reset now fails when a store cannot report its last id**
- `spindle-host adv_w31_u8_r2_a_failed_reset_keeps_the_letters_and_the_next_continues_past_them`: a read-only state directory makes the reset fail with `restored: true`. The letter keeps its id, and the next reset continues past it.
- My first attempt at this case tried to build a lost store (`restored: false`) the same way. It could not: the reset reopened the file and answered `restored: true`. That was a failed construction, not a finding, so I replaced it with the case above.

## 3. Suite run (after the cases existed; `adv-scratch/r2-suite.sh`, `r2-clippy.sh`)
```
test spindle-kernel exit 0    test result: ok. 474 passed; 0 failed; 55 ignored; ... finished in 8.28s
test spindle-host exit 0      test result: ok. 257 passed; 0 failed; 14 ignored; ... finished in 10.16s
test spindle-ingress exit 0   test result: ok. 170 passed; 0 failed; 0 ignored; ... finished in 10.90s
fmt spindle-kernel / spindle-host / spindle-ingress exit 0
clippy (all three) exit 3, twice: slot: connection to daemon lost
PostgreSQL lane: 9 tests run: 9 passed, 520 skipped   (pg-lane: the command exited 0)
```

## 4. Notes (no red case)
1. **Deviation 1 is not shown reachable.**
   - **What I read:** `crates/spindle-host/src/host.rs:1821` and `:1853` ask the old store for its last id before replacing it, so a store lost in an earlier reset (`Lost`, `restored: false`) can no longer be recovered by another reset.
   - **What a console user would see:** `resetting <component>: reading the last dead letter id: <component>'s store was lost in a reset: …; its store is as it was before the reset`. The new error is marked `restored: true`, so the message's ending says the store is as it was, without saying it is still lost.
   - **What reaches it:** a lost file store needs both the move aside and the reopen to fail. A read-only directory gives `restored: true`, and I found no other way to build it. Note, INFEASIBLE, introduced.
2. **`RefusedCommand::not_kept` is never shown.**
   - **What I read:** `crates/spindle-cli/src/serve.rs:610-650` prints a line (with the `not_kept` field) only for `Failed`. A refused command whose letter the store could not keep says nothing on stderr or in the log.
   - **What reaches it:** a store that fails to write the letter (PostgreSQL refusing U+0000 in the input, or a full disk). Note, CONFIRMED, introduced.
3. **The served `not_kept` field has no end-to-end test.**
   - **What I read:** only the pure function `did_not_run` has a test (serve.rs, `tests::a_dead_letter_its_store_could_not_keep_is_printed_dropped_with_why`). The `.field("not_kept", …)` line has none.
   - **What reaches it:** I found no way to make a store refuse a letter inside a served run. Note, CONFIRMED, introduced.

## 5. Attacked and not broken
- **F1:** resets in a row, a reset with no letters, a restart after a reset, a state move of a file with a continue event, the feed, scans, counts and folds skipping the continue event, two handles on one file and a reopened file all hold. Ids only grow: the offset never goes down and the version always goes up.
- **F2:** tables holding only dead letters, or only a continue event, are not empty.
- **F3:** the answers for ran, refused (escalating outcome) and failed are right and match the schema. `own` is taken before `settle`. I did not build a multi-process host to exercise the peer-envelope case.
- **F4:** the printed line is right for kept, not kept and plain dropped.

## 6. Paths written outside the worktree (this round)
All under `<local path>`:
- Scripts: `r2-cases.sh`, `r2-suite.sh`, `r2-clippy.sh`
- Commit message: `commit-r2.txt`
- Saved diff: `r2-impl.diff`
- Scratch copy: `host-head.rs`
- Case logs: `r2-cases-exit.txt`, `r2-host.log`, `r2-host-b.log`, `r2-kernel.log`, `r2-ingress.log`, `r2-pg.log`
- Suite logs: `r2-suite-exit.txt`, `r2-suite-spindle-kernel.log`, `r2-suite-spindle-host.log`, `r2-suite-spindle-ingress.log`
- Clippy logs: `r2-clippy-spindle-kernel.log`, `r2-clippy-spindle-host.log`, `r2-clippy-spindle-ingress.log`, `r2-clippy-exit.txt`, `r2-clippy2-spindle-kernel.log`, `r2-clippy2-spindle-host.log`, `r2-clippy2-spindle-ingress.log`
- fmt logs: `r2-fmt-spindle-kernel.log`, `r2-fmt-spindle-host.log`, `r2-fmt-spindle-ingress.log`

The round-1 files listed in my earlier report are still there.

## 7. Findings block
```findings
- file: crates/spindle-host/src/host.rs
  line: 1821
  category: judgement
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: a reset now asks a lost file store for its last dead letter id and fails marked restored true, so a store lost in an earlier reset is not recovered by a reset and the message does not say it is lost; I could not build the restored-false state that reaches this
- file: crates/spindle-cli/src/serve.rs
  line: 610
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: RefusedCommand::not_kept is never printed or logged, so a refused command whose dead letter the store could not keep leaves no trace
- file: crates/spindle-cli/src/serve.rs
  line: 652
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the served not_kept log field has no end-to-end test; only the pure function did_not_run is tested
```

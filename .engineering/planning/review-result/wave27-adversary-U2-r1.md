---
format: aep.planning-md/3
id: review-result:wave27-adversary-U2-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 27 U2
relations:
- reviews: story:seed-once-on-durable-state
- reviews: story:serve-loop-signal-exit-answers
revision: 1
---
```
unit: story:seed-once-on-durable-state, story:serve-loop-signal-exit-answers; branch unit/w27-serve, findings cover head 709b15aca (my tests committed on top as 89554e052)
verdict: NEEDS-CHANGE
cases: executed 500→506, red 3
origin: introduced 4 / pre-existing 0 / undecided 0
wrote-outside-worktree: 5 paths (part 6)
needs-coordinator: two decisions. Should a rejected seed entry count as done in seed.json (finding 1)? Should a reset on a --state with no --seed also reset seed.json (finding 3)? Finding 3 is the open gap the brief asked me to decide on.
```

**Result:** three of my cases fail on the unit's code; the other three pass and stay in as kept tests. The test that was previously ignored (adversary_w26_u3) passed 10 runs out of 10, so it is not flaky.

## 1. Diff stat (89554e052 against 709b15aca)
```
 crates/spindle-cli/tests/spindle-cli/seed.rs                  |   1 +
 crates/spindle-cli/tests/spindle-cli/seed/adversary_w27_u2.rs | 400 +++++
```
Both are test files. I changed no implementation file.

## 2. Cases added
All are in `<local path>`, with `mod adversary_w27_u2;` added in `seed.rs`. No test takes more than 5 s; the whole module runs in 5.07 s.

**Red:**
- **`a_restart_after_a_seed_that_rejected_an_entry_does_not_hide_the_rejection` (:103)**
  - Setup: a seed with one rejected entry (`not_granted`) on a `--state`. The first start exits 1.
  - On restart with the same seed, the run prints "already ran" and keeps serving. It never exits 1 again.
  - Red output: `left: None  right: Some(1)` … `"…seed.json (sha256:ee00…) already ran on the state …: no entry is sent" and kept serving`.
- **`a_resumed_seed_does_not_submit_a_request_id_applied_before_the_stop` (:151)**
  - Setup: seed `[grant seed-1, vouch v-1 seed-2, vouch v-1 seed-2, grant seed-3]`. SIGTERM arrives while entry 2 is held, so the marker records 2 done.
  - The restart resumes at entry 3. It has forgotten request id `seed-2`, so it submits the repeat. The repeat is rejected and the resumed run will exit 1.
  - Red output: `(asked again: true): … ended: 2 sent, 1 applied, 0 already there, 0 refused, 1 rejected`.
- **`a_seed_after_a_reset_by_a_run_without_it_applies_to_the_emptied_state` (:206)**
  - Setup: a start with `--seed`, then a start without `--seed` whose `POST /api/reset` empties the state, then a start with the same `--seed`.
  - The third start skips the seed, and the state stays empty.
  - Red output: `the state holds none of the seed's rows, yet the start skips the seed: …seed.json (sha256:7e6e…) already ran on the state …: no entry is sent`.

**Green, kept:**
- `a_reset_completed_after_the_signal_reads_503_and_leaves_the_state`: the reset's head is sent before SIGTERM and its last bytes after it. The client reads 503, the run exits 143 and the row survives. This targets the signal check after `listener.next` (serve.rs), which no unit test reaches. I did not run a mutated copy to prove the test catches its removal.
- `sigint_with_a_grant_queued_reads_503_and_exits_130`: queued grant reads 503, exit 130, marker records 2 done, state keeps only the seed's rows.
- `a_command_under_way_at_the_signal_is_answered`: a VouchFor sent over HTTP is held at the desk, then SIGTERM arrives. The client reads 2xx and the run exits 143.

## 3. Suite, run after the cases existed
`cargo test --locked -p spindle-cli --test spindle-cli --no-fail-fast` → exit 101
`test result: FAILED. 500 passed; 6 failed; 33 ignored; 0 measured; 0 filtered out; finished in 30.33s`
- Failures: my 3 red cases, plus the 3 `ui/dist` cases that are red on the base (`deploy::the_ui_is_served_from_the_binary`, `observe_panel::the_panel_may_be_framed`, `observe_panel::the_panel_is_one_self_contained_page`).
- Executed: 506 after, 500 before (the same run with my 6 deselected).
- adversary_w26_u3 run alone 10 times: 10 passed, each in 0.53 s.
- clippy `-p spindle-cli --tests -D warnings` is clean, and rustfmt check passes.

## 4. Findings (all on 709b15aca)
1. **crates/spindle-cli/src/seed.rs:590** (`self.marked()` counts a rejected entry as done). Contract drift with story:run-fails-on-rejected-line: the exit 1 now happens only on the first start. A supervisor restarting after the exit 1 gets a run that serves normally, and the entry that never ran is never retried. The same happens when SIGTERM follows a rejected entry before the seed ends. **NEEDS-CHANGE, introduced.** On the base, the restart re-sent the entry and exited 1 again.
   - What reaches it: any `run --seed --state` whose seed has an entry rejected at run time, restarted by hand or by a supervisor.
2. **seed.rs:228 / :409** (a resume starts with an empty `submitted` set). After a SIGTERM stop, an entry whose request id an earlier entry already applied is submitted again. In my case the repeat is rejected, so a seed that runs cleanly in one start fails with exit 1 after a stop and restart. **NEEDS-CHANGE, introduced.**
   - The resume path is new. The base also double-sent here, by replaying every entry, so the base would be red on the same assertion.
   - What reaches it: SIGTERM during any seed that repeats a request id, a documented feature that `a_second_run_on_one_state_applies_nothing_new` relies on.
   - Fix to name, not apply: record the applied request ids in seed.json, or rebuild them from entries 0..done.
3. **crates/spindle-cli/src/serve.rs:488** (a reset in a run without `--seed` goes through `serve_with` and never touches seed.json). seed.json then says every entry is done on a state that holds no row. **CONFIRMED, warning, introduced.** On the base, the third start re-seeded the empty state.
   - The decided text ("same digest, every entry done: send nothing") allows this literally. It conflicts with the story's rule that a seed with another digest re-runs on a state that holds no row, and with D1's idea that the marker tracks what the state holds.
   - Decision for the coordinator. Options: any reset on a `--state` zeroes or removes seed.json, or a same-digest marker on a state with no row counts as not done.
4. **seed.rs:171 / :185** (judgement): the marker is rewritten in place (truncate, write, no rename, no fsync) after every entry. A write cut at the wrong moment leaves either an empty file, which reads as no marker so the whole seed replays on a seeded state, or a partial file, which stops every start with "does not hold". Writing a temp file and renaming it would close this. **INFEASIBLE, note, introduced.** Only SIGKILL or a crash inside that short window reaches it; I built no case.

## 5. Attacked and not broken
- Queued request at SIGTERM or SIGINT reads 503, with exit 143 or 130.
- A command under way at the signal is answered.
- A reset completed after the signal reads 503 and the state is untouched.
- An answer handed to the listener before close is written (unit's close.rs test, plus my 503 probe).
- adversary_w26_u3 is stable over 10 runs.
- hyper graceful shutdown: a newly accepted connection is not idle, so a half-sent request head still gets 503 rather than EOF.
- Exit codes 143, 130 and 1.
- Changed digest with rows present: exit 1 before listening (unit's test).
- A state with rows and no marker (unit's test).
- `file` vs `sqlite` `--state`: only sqlite takes `--state`; memory and postgres refuse it, so there is nothing more to attack there.

## 6. Paths written outside the worktree
All in `<local path>`: `build.log`, `runs.sh`, `w26.log`, `suite.log`, `commit-msg.txt`.

## 7.
```findings
- file: crates/spindle-cli/src/seed.rs
  line: 590
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a rejected seed entry is counted done in seed.json, so a restart prints "already ran" and serves, never resending the entry or exiting 1 as story:run-fails-on-rejected-line requires
- file: crates/spindle-cli/src/seed.rs
  line: 228
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a seed resumed after SIGTERM forgets the request ids applied before the stop, submits a repeat of one, and the resumed run rejects it and exits 1
- file: crates/spindle-cli/src/serve.rs
  line: 488
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a reset by a run without --seed empties a --state but leaves seed.json at every entry done, so a later start with the same --seed skips the seed on the empty state
- file: crates/spindle-cli/src/seed.rs
  line: 185
  category: judgement
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: seed.json is rewritten in place without rename or fsync, so a write cut short leaves an empty marker (the whole seed replays) or a partial one (every start stops)
```

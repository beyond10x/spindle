---
format: aep.planning-md/3
id: review-result:wave31-adversary-U6-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 31 U6
relations:
- reviews: story:several-writers-one-postgres
revision: 1
---
unit: story:several-writers-one-postgres, round 2. Findings cover head 9ecb1beb, which is d74d326d plus my test commit.
verdict: CONFIRMED
cases: executed 120→127, red 1
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 8 new paths in the U6 adv-scratch directory (part 6)
needs-coordinator: none

F2, F3 and F4 hold, and so does F1 for a paused leader. F1 still fails in one case: a running leader whose lease database connection is dropped. That frees the lease at once, and the leader answers commands with the fence's 501 instead of 503 until its next renewal. All 4 round-1 red cases are now green.

**1. Diff stat (d74d326d..HEAD).** One test file, no other path.
```
 .../tests/spindle-cli/adversary_w31_u6.rs          | 319 +++++++++++++++++++++
```

**2. Cases added** (same module `adversary_w31_u6::postgres`, each run alone in the lane before the suite)
- **`a_leader_whose_lease_session_ended_answers_a_command_503`: red, 3 of 3 runs.**
  - Setup: commands are sent to the leader back to back; the test terminates its lease connection with `pg_terminate_backend`, and the standby takes the lease.
  - Assertion: every command is answered 201 or 503.
  - A first version sent one command after the standby's take line. It passed only because the old leader had already exited (status 0), so I rewrote it to send commands continuously.
  - Output, one of the three runs:
  ```
  the old leader answered 556 commands: 272 x 201, 1 x 503
  the old leader answers a command after losing its lease neither 201 nor 503: ["HTTP/1.1 501 Not Implemented ... {\"committed\":false,\"refused\":\"store: fenced: the lease of board was taken by another process (token 2, this process took token 1), so this process writes nothing more to its tables\"}", ...
  ```
  The other two runs: 500 commands (237 x 201, 1 x 503) and 545 (270 x 201, 1 x 503). Each time about 260 to 280 commands got the 501.
- **`a_leader_whose_renewal_is_held_up_still_answers`: green.** The lease row is locked for 1 s (half the lease), standing in for a slow database. A command is still answered 201 in under 3 s, the leader keeps running, and the standby stays on standby.
- **`a_pause_under_two_thirds_of_the_lease_answers_consistently`: green, 3 of 3 runs.** The leader is paused for 1.3 s of a 2 s lease. A command sent during the pause gets 201 with no takeover, or 503 only after the standby took over. No false 503 and no other status.
- **`the_shortest_lease_leads_and_fails_over`: green.** With a 1 s lease, 7 commands 300 ms apart all get 201. After SIGKILL the standby takes over and answers.
- **`the_longest_lease_leads`: green.** A 86400 s lease leads and answers; a second process stands by. The 86401 refusal is already in the unit's own test.
- **`a_seed_with_one_lease_taken_before_is_skipped_naming_it`: green.** The test takes and releases `ledger`'s lease first, so the run gets board at token 1 and ledger at token 2. The seed is skipped and the log line names `lease of ledger at token 2`.
- **`a_restart_after_a_clean_stop_skips_the_seed`: green.** After a SIGTERM (exit 143) and a restart, the run takes token 2, logs that the seed is not submitted, and keeps serving.

**3. Suite, run after the cases existed**
- `slot --timeout 30m sh ./.pglane.run.sh task check:postgres`: `Summary [21.233s] 127 tests run: 126 passed, 1 failed, 1463 skipped`. Within reports `21.682 s of 120 s`; the harness exited 201. The one failure is the red case above. "Before" is 120: the same run with my 7 round-2 cases left out.
- `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings`: exit 0.
- `cargo fmt -p spindle-cli --check`: exit 0.

**4. Findings**

**`crates/spindle-cli/src/database.rs:447` (`Kept::confirmed`, called at `serve.rs:657` and `:702`): contract drift, warning, CONFIRMED, introduced.**
- What happens: `confirmed` decides only from how old the last renewal is. A lease is also free once its holder's session ends, so a standby can take it from a leader that is still running and renewed recently. Until the leader's next renewal pass, which can be up to a third of the lease away, every command it takes is refused by the fence with 501. D4 says 503.
- What reaches it: the lease connection of a live leader ending. That happens with `pg_terminate_backend`, a dropped TCP session, a server idle-session timeout, or the pooler case the docs already name. The fence holds throughout; no write lands under a lost lease.
- Possible fix: treat the fence's refusal (`GuardRefused` "fenced") of a command as a lost lease: answer 503, log, exit 1.

Judgement on F4, not a defect: the code now matches D5 read literally. With `database_lease` the seed runs only while every lease is at token 1. The docs say so, including a restart of the only process.
- The consequences go beyond D5's wording. The seed never runs again on those tables, including a changed seed file in a later release. It also never runs if the first token-1 process died before seeding, for example by exiting 3 on a misfit after taking the leases.
- This narrows `story:seed-at-start`'s "on every start". It is a planning question for the coordinator, not something to fix in this unit.

**5. Attacked and could not break**
- F1 with a pause just under two thirds of the lease, and with a held-up renewal: neither shows a false 503 or a latency problem.
- F1 with a pause longer than the lease: still 503 (the unit's tightened test plus my round-1 case).
- F2: a lock held for 1.5 s leaves the standby standing by (round-1 case, now green).
- F3: bounds 1 and 86400 both lead. Above 86400 is refused before connecting.
- F4: mixed tokens across components, a restart after a clean stop, and a restart after a kill all skip the seed and log the lease and token.
- The lane step stays at 21.7 s of 120 s.

**6. Paths written outside the worktree** (all under `<local path>`)
- `r2-run1.log`
- `r2-run2.log`
- `r2-run3.log`
- `r2-lane.log`
- `r2-clippy.log`
- `commit2.txt`
- `head.rs` and `tail.rs`, written while splitting the file and then deleted.

The round-1 files from my first report are still there: `run1.log` to `run4.log`, `lane.log`, `clippy.log`, `commit.txt`.

**7. Findings block**
```findings
- file: crates/spindle-cli/src/database.rs
  line: 447
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a live leader whose lease session ended loses the lease with a recent renewal, so the age-only check passes and every command it takes until its next renewal pass is answered 501 with the fence's refusal instead of D4's 503
```

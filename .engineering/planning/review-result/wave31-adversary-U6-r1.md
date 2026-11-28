---
format: aep.planning-md/3
id: review-result:wave31-adversary-U6-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 31 U6
relations:
- reviews: story:several-writers-one-postgres
revision: 1
---
unit: story:several-writers-one-postgres (wave 31 U6). Findings cover head faa65b0b, which is 9a0210cf plus my test commit.
verdict: CONFIRMED
cases: executed 113→120, red 4
origin: introduced 4 / pre-existing 0 / undecided 0
wrote-outside-worktree: 7 paths, all in the U6 adv-scratch directory (part 6)
needs-coordinator: none

Four new tests fail against the unit's code:
- A command queued at a paused leader gets 501 back, not the documented 503.
- A standby crashes while the leader holds the lease row for longer than a third of the lease.
- A `database_lease` of 10,000,000 s is refused only after connecting, and the message never names the setting.
- A process that leads under the lease's second token submits the seed again and exits 1. D5 says the seed runs only under the first token.

**1. Diff stat (9a0210cf..HEAD).** Both paths are test files.
```
 .../tests/spindle-cli/adversary_w31_u6.rs          | 610 +++++++++++++++++++++
 crates/spindle-cli/tests/spindle-cli/main.rs       |   1 +
```

**2. Cases added**

The file is `crates/spindle-cli/tests/spindle-cli/adversary_w31_u6.rs`, module `postgres`, so the lane's `postgres::` filter picks it up. I ran each case alone in the lane, with the U1 harness copied in as `.pglane.run.sh` and removed afterwards.

- **`a_resumed_leader_answers_its_queued_command_503`: red.** It checks D4 and the docs' sentence "answers the requests already queued 503". Output:
  ```
  assertion `left == right` failed: the command queued at the paused leader is answered 503: HTTP/1.1 501 Not Implemented
  {"committed":false,"refused":"store: fenced: the lease of board was taken by another process (token 2, this process took token 1), so this process writes nothing more to its tables"}
  b10x-spindle: another process took the lease of board, so this one writes nothing more and exits
  ```
- **`a_standby_outlives_a_leader_commit_longer_than_a_third_of_the_lease`: red.** The test holds the lease row `FOR UPDATE` for 1.5 s, the same row lock the fence takes for a leader's whole commit. The standby should still be running and still answer 503 with `standby: true`. Output:
  ```
  the standby exited while the leader held the lease row for 1.5 s: b10x-spindle: standing by: ...
  b10x-spindle: store: PostgreSQL refused a statement on the lease of board: canceling statement due to statement timeout (SQLSTATE 57014)
  ```
- **`a_long_lease_is_admitted_or_refused_by_name`: red.** `database_lease=10000000` is a whole number above 0, so the run should either lead or exit naming `database_lease`. Output:
  ```
  b10x-spindle: connecting for the leases: store: PostgreSQL refused a statement on the lease of the lease: invalid value for parameter "statement_timeout": "3333333333" (SQLSTATE 22023)
  ```
- **`the_seed_runs_only_under_the_first_token`: red.** It runs a seeded leader, kills it, then starts a new process, which takes token 2 at start. It expects the seed to be skipped with a log line, as D5 A says. Output:
  ```
  the process leading under the second token skips the seed: b10x-spindle: seed <seed>: 1 entries, submitted one at a time once listening
  b10x-spindle: seed <seed> entry 1: tasks.board.OpenTask (request id seed-1) rejected: undeclared
  b10x-spindle: seed <seed> rejected an entry, so the run exits 1
  ```
- **`a_resumed_leader_posts_no_event_the_new_leader_committed`: green, 4 of 4 runs.** The paused leader's egress posts nothing once it resumes; `t2` is posted once, by the new leader.
- **`two_standbys_race_for_one_released_lease`: green.** One standby leads; the other stays on standby and keeps running; each write lands once.
- **`a_killed_leaders_lease_is_free_before_it_runs_out`: green.** With a 9 s lease, the standby takes the lease within 5.5 s, before it could have run out. This guards the "free once the session ends" part of D9. The unit's own 2 s / 5 s test passes even if that check is dropped from `TAKE`. I did not build and run that mutant; the claim rests on the timing arithmetic.

Times: every new case runs in under 6 s. The egress case took 5.5 s on its first run and 4.1 s afterwards.

**3. Suite, run after the cases existed**
- `slot --timeout 30m sh ./.pglane.run.sh task check:postgres`: `Summary [24.899s] 120 tests run: 116 passed, 4 failed, 1463 skipped`. Within reports `31.090 s of 120 s`; the harness exited 201. The 4 failures are exactly the red cases above. "Before" is 113: the same run with my 7 cases left out.
- `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings`: exit 0.
- `cargo fmt -p spindle-cli --check`: exit 0.

**4. Findings** (all at faa65b0b)

1. **`crates/spindle-cli/src/serve.rs:653`: contract drift, blocker, CONFIRMED, introduced.**
   - What happens: the lease-lost check runs only at the top of the loop. On SIGCONT the host thread takes the queued exchange first. The fence refuses the write, so the client gets 501 "Not Implemented" with the store error.
   - What reaches it: any leader paused past its lease (the story's SIGSTOP scenario) with requests waiting in its socket.
   - The unit's `a_paused_leader_is_fenced` (`replicas.rs:469`) only asserts "not 2xx", so it passes on a 501.
   - Possible fix: check `Kept::lost` (or the fence's refusal) before running an exchange and answer 503.
2. **`crates/spindle-cli/src/serve.rs:879` (and the `take` at the start of `serve`): concurrency, warning, CONFIRMED, introduced.**
   - What happens: `leases.take()?` turns a lock or statement timeout on the lease row into a fatal error, and the standby exits 1. The timeout is set to a third of the lease (`crates/spindle-kernel/src/postgres.rs:398`).
   - What reaches it: the fence holds the row with `get_for_update` until commit. Any leader commit longer than a third of the lease does this; with the default 10 s lease that is 3.3 s, and the docs mention commands that write 20,000 instances.
   - The lock in the test is taken by hand, not by a real long commit.
   - Possible fix: treat a lock or statement timeout during a standby poll as "still held".
3. **`crates/spindle-kernel/src/postgres.rs:398`: boundary, note, CONFIRMED, introduced.**
   - What happens: a lease above about 6,442,450 s overflows `statement_timeout`. The run is refused after connecting, and the message reads "lease of the lease" without naming `database_lease`.
   - What reaches it: only an implausibly long lease setting.
   - Possible fix: give the setting an upper bound in `lease_term` (`database.rs:248`), refused before connecting.
4. **`crates/spindle-cli/src/serve.rs:573`: acceptance, warning, CONFIRMED, introduced.**
   - What happens: the seed is skipped only when this process stood by first. D5 A says the seed runs only under the first token. A leader restarted by its supervisor after a kill, or any later deploy, submits the seed again. A seed entry that is not `existing_instance:` is then rejected and the run exits 1, so the supervisor restarts it into the same failure.
   - Re-seeding on every start already happened at the base; what this unit adds is falling short of D5. The unit's docs restate the rule as "the one that leads from its start".

One note left as text: D9 calls the lease "a session advisory lock". The code instead checks the holder's backend PID in `pg_stat_activity`. That is a different mechanism with the same visible effect (the lease frees once the holder's session ends). I am not raising it as a defect.

**5. Attacked and could not break**
- Two standbys racing for one lease: one leads.
- Takeover after SIGKILL frees the lease through the session end.
- The paused leader's commit lands nothing; the head is unchanged.
- The paused leader's egress posts no duplicate (4 runs).
- A standby answers every route 503 and writes nothing.
- Leases taken in component order: no deadlock path found with one specification. Only mismatched component sets could reach one.
- Every kernel write path goes through the fenced `grouped` append. The egress handles are fenced too, through the process-wide list of held leases.
- The role-limit shortfall (pools plus the lease connection exceed the most eventlog allows) is documented as a known limit, so not raised.

**6. Paths written outside the worktree** (all under `<local path>`)
- `run1.log`
- `run2.log`
- `run3.log`
- `run4.log`
- `lane.log`
- `clippy.log`
- `commit.txt`

**7. Findings block**
```findings
- file: crates/spindle-cli/src/serve.rs
  line: 653
  category: contract-drift
  severity: blocker
  verdict: CONFIRMED
  origin: introduced
  message: a command queued at a leader paused past its lease is answered 501 with the fence's store refusal, where D4 and the documents promise 503, because the lost-lease check runs only before the next wait, after the queued exchange was already taken
- file: crates/spindle-cli/src/serve.rs
  line: 879
  category: concurrency
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a standby exits 1 on a lock or statement timeout of its lease poll when the lease row stays locked longer than a third of the lease, as a long leader commit holds it through the fence's get_for_update
- file: crates/spindle-kernel/src/postgres.rs
  line: 398
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a database_lease above about 6442450 s passes the setting check and is refused after connecting with an out-of-range statement_timeout, the message naming "the lease of the lease" and not database_lease
- file: crates/spindle-cli/src/serve.rs
  line: 573
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a process leading under the lease's second token without standing by first submits the seed again, against D5 A's "only under the first token", and exits 1 when an entry is rejected as a duplicate
```

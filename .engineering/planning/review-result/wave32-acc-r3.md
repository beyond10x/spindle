---
format: aep.planning-md/3
id: review-result:wave32-acc-r3
kind: review-result
status: active
title: Plan critic (acceptance), round 3, on wave 32
relations:
- reviews: story:postgres-lane-runs-locally
- reviews: story:conform-awaits-system-steps
- reviews: story:leader-exits-promptly-on-sigterm
- reviews: story:seed-under-a-lease
revision: 1
---
needs-revision

- story:postgres-lane-runs-locally — the clean-up promise "also when a test fails or the run gets SIGINT" rests on an assertion that the same task runs "at its end", which SIGINT interrupts, and no check or red-first case exercises a failing run or an interrupted one — .engineering/planning/story/postgres-lane-runs-locally.md:44-47
- story:postgres-lane-runs-locally — the Outcome promises that the script prints its major version, warns off 17 and honours `SPINDLE_PG_BIN` (D3 A), but no acceptance line observes any of the three — .engineering/planning/story/postgres-lane-runs-locally.md:30-31
- story:postgres-lane-runs-locally — the wave page adds a duty the story does not hold: record the `pg_stat_activity` peak and raise `max_connections` if it comes within 50, so the story can close without it — .engineering/waves/wave-32.md:68-72
- story:conform-awaits-system-steps — "`conform` over the fixture reports 0 skipped, and `conformance.toml` keeps `skipped = 0`" is two statements, sits in a list headed "All tests below" with no test name, and its second half already holds before the work (`conformance.toml:88` says `skipped = 0`, and D5 A adds no await scenario to Spindle's own suite) — .engineering/planning/story/conform-awaits-system-steps.md:60
- story:conform-awaits-system-steps — the fixture is described only as "a command every grant of which is a `system: true` actor", so it need not hold an optional input, a stated input or a `when_related:` input, and a host that mints every input would pass the parity line against D1 A's input order — .engineering/planning/story/conform-awaits-system-steps.md:49-55
- story:conform-awaits-system-steps — the Outcome promises `run --scenario` runs an awaiting scenario, but every acceptance line goes through `conform` or `HostTarget` and none replays a scenario — .engineering/planning/story/conform-awaits-system-steps.md:28
- story:leader-exits-promptly-on-sigterm — the Outcome says each release at exit waits at most 1 s per statement whatever `database_lease` is (D3 A), and no acceptance line observes that bound — .engineering/planning/story/leader-exits-promptly-on-sigterm.md:45-46
- story:leader-exits-promptly-on-sigterm — the Outcome says every cursor a lane handed back is saved before the leases are released, and the end-to-end test names no assertion that t1 is not posted again after the takeover — .engineering/planning/story/leader-exits-promptly-on-sigterm.md:44-46
- story:leader-exits-promptly-on-sigterm — the end-to-end bound is "the bound `run --help` names", which the Outcome says is "measured and adjusted before merge" and which is drain + 2 s + 5 s with the test's drain and `timeout_ms` unstated, so the pass line moves with the implementation, and the hold can only outlast it while the bound stays under the 10 s cap — .engineering/planning/story/leader-exits-promptly-on-sigterm.md:65-66
- story:leader-exits-promptly-on-sigterm — "within two thirds of the lease of that exit" cannot be read the same way twice, because the acceptance names neither the lease value nor the instant the two thirds are counted from — .engineering/planning/story/leader-exits-promptly-on-sigterm.md:67-68
- story:leader-exits-promptly-on-sigterm — the first bullet joins a documentation outcome (the wait "named in the story's closing record", no location given) to a kernel test that is "red first on 0.26.0", and a stopped `events_placed_after` has no form on 0.26.0 to be red with — .engineering/planning/story/leader-exits-promptly-on-sigterm.md:58-62
- story:seed-under-a-lease — the Outcome says resets write and delete the record (D6 A), and none of the six named tests resets a leased PostgreSQL run — .engineering/planning/story/seed-under-a-lease.md:51-52
- story:seed-under-a-lease — the Outcome says the lease thread writes the record only while the lease row names this process's token (D3 A), and no test shows a deposed process failing to write — .engineering/planning/story/seed-under-a-lease.md:48-49
- story:seed-under-a-lease — `another_seed_file_is_refused_before_serving` holds two independent outcomes (exit 1 naming both digests, and "on a store that holds no row it seeds from the first entry"), so one can pass while the other fails — .engineering/planning/story/seed-under-a-lease.md:76-77

Read: five artifacts (the four stories and `wave-32.md`, with `cat -n`), plus `critic-rubric.md`, ESS 0.99.2 `runner/await_outcome.rs`, `Taskfile.yml`, `.config/nextest.toml`, `postgres-lane-margin.md`, and the cited `replicas.rs`, `adversary_w31_u6.rs`, `seed.rs`, `serve.rs` and `sqlite.rs` ranges. I did not run `aep plan artifact show`, `kinds` or `lifecycle`; the bodies came from the files directly.

Could not establish:
- Whether the kernel red-first test can fail on 0.26.0, since the cause named is unproven and the stop API does not exist there.
- How `a_killed_leader_resumes_the_seed_after_its_count` holds entry 2 so the kill lands after entry 1. The state-directory tests use a holding desk, but the PostgreSQL tests have no such fixture.
- Out of my lane, not counted in the verdict: `story:postgres-lane-margin` records the lane at 112.3 s of 120 s on CI, and U3 and U4 add 8 process tests to it, one of about 9 s. That is a coupling and budget matter for the other critics.

```findings
- file: .engineering/planning/story/postgres-lane-runs-locally.md
  line: 44
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the clean-up promise 'also when a test fails or the run gets SIGINT' rests on an assertion the task runs 'at its end', which SIGINT interrupts, and no check or red-first case exercises a failing run or an interrupted one"
- file: .engineering/planning/story/postgres-lane-runs-locally.md
  line: 30
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome promises that the script prints its major version, warns off 17 and honours SPINDLE_PG_BIN (D3 A), and no acceptance line observes any of the three"
- file: .engineering/waves/wave-32.md
  line: 68
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the wave page adds a duty the story's acceptance does not hold (record the pg_stat_activity peak, raise max_connections if it comes within 50), so story:postgres-lane-runs-locally can close without it"
- file: .engineering/planning/story/conform-awaits-system-steps.md
  line: 60
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the 0-skipped line is two statements in a list headed 'All tests below' with no test name, and its conformance.toml half (skipped = 0) already holds before the work, since D5 A adds no await scenario to the repository's own suite"
- file: .engineering/planning/story/conform-awaits-system-steps.md
  line: 49
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the fixture is described only as a command every grant of which is a system actor, so it need not hold an optional, a stated or a when_related input, and a host that mints every input would pass the parity line against D1 A's input order"
- file: .engineering/planning/story/conform-awaits-system-steps.md
  line: 28
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome promises run --scenario runs an awaiting scenario, and every acceptance line goes through conform or HostTarget with none replaying a scenario"
- file: .engineering/planning/story/leader-exits-promptly-on-sigterm.md
  line: 45
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the Outcome says each release at exit waits at most 1 s per statement whatever database_lease is (D3 A), and no acceptance line observes that bound"
- file: .engineering/planning/story/leader-exits-promptly-on-sigterm.md
  line: 44
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome says every cursor a lane handed back is saved before the leases are released, and the end-to-end test names no assertion that t1 is not posted again after the takeover"
- file: .engineering/planning/story/leader-exits-promptly-on-sigterm.md
  line: 65
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the end-to-end bound is whatever run --help names, which the Outcome says is measured and adjusted before merge, and the test's drain and timeout_ms are unstated, so the pass line moves with the implementation and the 10 s hold can outlast it only while the bound stays under 10 s"
- file: .engineering/planning/story/leader-exits-promptly-on-sigterm.md
  line: 67
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "'within two thirds of the lease of that exit' cannot be read the same way twice, because it names neither the lease value nor the instant the two thirds are counted from"
- file: .engineering/planning/story/leader-exits-promptly-on-sigterm.md
  line: 58
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the first bullet joins a documentation outcome (the wait named in a 'closing record' with no location) to a kernel test 'red first on 0.26.0' that uses a stop that has no form on 0.26.0"
- file: .engineering/planning/story/seed-under-a-lease.md
  line: 51
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the Outcome says resets write and delete the record (D6 A), and none of the six named tests resets a leased PostgreSQL run"
- file: .engineering/planning/story/seed-under-a-lease.md
  line: 48
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome says the lease thread writes the record only while the lease row names this process's token (D3 A), and no test shows a deposed process failing to write"
- file: .engineering/planning/story/seed-under-a-lease.md
  line: 76
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "another_seed_file_is_refused_before_serving holds two independent outcomes (exit 1 naming both digests, and seeding from the first entry on a store with no row), so one can pass while the other fails"
```
needs-revision

All 14 round-1 findings are resolved, and one new finding is open. I checked each against the revised bodies at `b7f11c5a`.

- story:leader-exits-promptly-on-sigterm — the end-to-end test flips the webhook to `200` at about T0 and sends SIGTERM at most 2 s after T0, but it never says SIGTERM waits until the `200` post has landed and the cursor is saved, and the lane's retry pause doubles from 100 ms up to 5 s (`BACKOFF_CAP`), so the post can arrive after SIGTERM, the cursor then saves at exit, and the hold-outlasts-the-bound arithmetic does not fix the order the Outcome promises — .engineering/planning/story/leader-exits-promptly-on-sigterm.md:69-72

Round-1 findings, each checked against the revised text:
- **`story:postgres-lane-runs-locally`:** the clean-up, version and `max_connections` points are now `scripts/postgres-lane.test.sh` and the options check, red first on a script copy without its `trap` and on a `max_connections=200` copy. The peak is a closing-record item (story :47 ff., wave page :93). Resolved.
- **`story:conform-awaits-system-steps`:**
  - The 0-skipped line is split out as `no_await_scenario_of_the_fixture_is_skipped`, red on 0.26.0.
  - The fixture now holds a stated, an `Optional` and a `when_related:` input.
  - `a_replayed_scenario_runs_its_await` covers `run --scenario`.
  - Resolved.
- **`story:leader-exits-promptly-on-sigterm`:**
  - Resolved: the 1 s release test, `a_release_under_a_long_lease_waits_at_most_one_second`.
  - Resolved: t1 is not posted again.
  - Resolved: the bound numbers (7 s, 1.33 s, 10 s hold).
  - Resolved: the `CHANGELOG.md` entry now names the wait.
  - Resolved: the kernel stop test is marked "not red first".
  - Open: the SIGTERM timing, in the finding above.
- **`story:seed-under-a-lease`:** the reset tests, `a_deposed_process_cannot_write_the_seed_record`, and the split of the refusal case into two tests are in. Resolved.

What I read: five artifacts (the four stories and `wave-32.md`), via `git diff c5c207b4 b7f11c5a` per story plus the full leader story and the wave page. I also checked `egress.rs:48-58`, the ESS 0.99.2 interpreter's await table, and the existence of `replay.rs`. I ran no `aep plan artifact` verbs.

Could not establish:
- How `a_reset_of_a_leased_run_rewrites_the_record` observes `done` 0 "before the first entry is sent", since no pause fixture is named for the PostgreSQL tests. The same holds for the SIGKILL-after-entry-1 test, so I note it without counting it.
- Whether the new fixture's candidate combinations reach different accepting branches. If they do, the interpreter answers `unsupported` where the host passes (D2 A), and the parity line would fail.
- Out of my lane: the lane's budget margin of 112.3 s against 120 s is now handled as a timing duty in the wave page (:98-103).

```findings
- file: .engineering/planning/story/leader-exits-promptly-on-sigterm.md
  line: 69
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the end-to-end test sends SIGTERM at most 2 s after T0 without waiting for the 200 post and the cursor save, and the egress retry pause reaches 5 s, so the post can land after SIGTERM and the test no longer checks that the cursor is saved before the leases are released"
```
needs-revision

Round 3 fixes my round-2 finding. The end-to-end test now sends SIGTERM only after the webhook has received t1's `200` and fails when S is more than 5 s after T0. The numbers agree in the story (:70-81), D4 A (:119) and the wave page (:36, :98). One new finding is open: the wave page now sets a duty for U3 and U4 that neither story holds.

- story:leader-exits-promptly-on-sigterm — the wave page makes U3 sample the `pg_stat_activity` peak of one lane run and raise `max_connections` in script and CI when the peak is within 50, and the story's acceptance names neither the sample nor the raise, so the story can close without them (it has no such line, unlike `story:postgres-lane-runs-locally`) — .engineering/waves/wave-32.md:102-105
- story:seed-under-a-lease — the same wave-page duty binds U4, and the story's acceptance names neither the peak sample nor the raise of `max_connections` — .engineering/waves/wave-32.md:102-105

Round-1 and round-2 findings: none of the 15 reappears in the four stories at `b72e8ec8`, and the round-2 finding (SIGTERM timing against the `200` post) is fixed as described above. I checked the new leader-story text (:62-85) against `BACKOFF_CAP` = 5 s at `egress.rs:53`, and found no leftover "10 s hold" wording.

What I read: four stories and `wave-32.md` at `b72e8ec87`, using `git diff b7f11c5a b72e8ec87` over the stories and the wave page, plus the full revised acceptance of the leader story. I ran no `aep plan artifact` verbs.

Could not establish:
- Whether the cursor save lands before SIGTERM. The test observes the webhook's receipt of the post, not the save (the save is "between two exchanges" on the run's thread), so the story's "before SIGTERM" ordering rests on a millisecond window. The exit assertions still observe the outcome, so I do not count this.
- How `a_reset_of_a_leased_run_rewrites_the_record` observes `done` 0 "before the first entry is sent" (open since round 2).
- Out of my lane, not counted: a raise of `max_connections` by U3 or U4 edits U1's script and CI step, which is the parallel-safety critic's surface.

```findings
- file: .engineering/waves/wave-32.md
  line: 102
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the wave page makes U3 sample the pg_stat_activity peak of one lane run and raise max_connections when the peak is within 50, and story:leader-exits-promptly-on-sigterm's acceptance names neither, so the story can close without them"
- file: .engineering/waves/wave-32.md
  line: 102
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the same wave-page duty binds U4, and story:seed-under-a-lease's acceptance names neither the peak sample nor the raise of max_connections"
```

---
format: aep.planning-md/3
id: review-result:wave28-adversary-U4-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 28 U4
relations:
- reviews: story:seed-readiness
- reviews: story:health-and-graceful-drain
revision: 1
---
Three of my new cases fail on head `a0ae690b`, and two of those failures need code changes before U4 merges.

```
unit: story:seed-readiness, story:health-and-graceful-drain — branch unit/w28-health, findings cover a0ae690b (implementation at ab39655c)
verdict: CONFIRMED (2 NEEDS-CHANGE, 1 CONFIRMED, 1 INFEASIBLE)
cases: executed 725→732, red 3 (plus the 3 cases already red on the base, unchanged)
origin: introduced 4 / pre-existing 0 / undecided 0
wrote-outside-worktree: 1 directory (part 6)
needs-coordinator: none
```

**1. Files I changed** (`git --no-pager diff --stat ab39655c a0ae690b`). All are test files.
```
 crates/spindle-cli/tests/spindle-cli/seed.rs       |   1 +
 .../tests/spindle-cli/seed/adversary_w28_u4.rs     | 387 +++++++++++++++++++++
 .../tests/spindle-ingress/adversary_w28_u4.rs      | 115 ++++++
 .../spindle-ingress/tests/spindle-ingress/main.rs  |   1 +
```
They are committed on `unit/w28-health` as `a0ae690b` through `b10x-gates bot -- commit`. The scan for private names printed nothing.

**2. Cases I added.** I ran each one alone, right after writing it and before any suite run.

In `crates/spindle-cli/tests/spindle-cli/seed/adversary_w28_u4.rs`:
- **`a_plain_reset_never_reads_ready_before_its_seed_runs_again`: red.** Four probes keep reading `/__spindle/ready` and `/__spindle/seed` from the moment a reset is queued until the reset's seed holds again. It failed in round 0 of 5 out of 5 runs:
  ```
  the seed read `ended`, readiness 200, after the reset was queued and before its seed ran again: [
      "round 0: 200 {\"already_there\":0,\"applied\":2,\"entries\":3,\"left\":0,\"refused\":0,\"rejected\":0,\"sent\":2,\"state\":\"ended\"}",
      "round 0: 200 {\"ready\":true,\"seed\":\"ended\",\"stopping\":false}",
  ```
  `sent: 2` with `entries: 3` are the counts of the run the reset cut. The test also checks that the first end line reads `: 2 sent,`, so the start's seed never ended by itself and this 200 cannot come from it.
- **`a_reset_taken_in_the_drain_answers_with_its_seed_in_place`: red.**
  ```
  panicked at .../adversary_w28_u4.rs:296:5:
  the reset queued at SIGTERM answered Some(200) and left the state holding []: its stores emptied and its seed never sent again
  ```
- **`a_drain_bound_past_the_clock_still_exits_143`: red.**
  ```
  "thread 'main' (1742198) panicked at crates/spindle-cli/src/serve.rs:503:44:",
  "overflow when adding duration to instant",
    left: Some(101)
   right: Some(143)
  ```
- **`every_health_route_answers_403_without_the_proxy_secret`: green, kept.** On a `--trusted-caller-header` run, `live`, `ready` and `seed` each answer 403 with no secret and with a wrong one, and 200 with the right one.
- **`sigint_with_the_default_drain_answers_the_queued_request_and_exits_130`: green, kept.** The held request and the queued one both get their outcomes, and the run exits 130.

In `crates/spindle-ingress/tests/spindle-ingress/adversary_w28_u4.rs`:
- **`a_body_finished_after_the_stop_reads_503_and_is_never_handed_over`: green, kept.** It targets the mutant that drops the stopping check in `handed`. I did not build a mutated copy, so it is not proven to kill that mutant.
- **`the_health_routes_answer_once_stopping`: green, kept.** After the stop, ready answers 503 with `{"ready":false,"seed":"none","stopping":true}`, seed answers `none`, live answers 200, and `/api/model` answers 503.

Every case takes under 1 s.

**3. Suite runs**, after the cases existed.
- `cargo test --locked -p spindle-ingress --no-fail-fast`: exit 0, 21 + 154 passed, 16 s wall.
- `cargo test --locked -p spindle-cli --test spindle-cli --no-fail-fast`: exit 101, `551 passed; 6 failed; 33 ignored`, 30.22 s, 43 s wall. The 6 failures are my 3 red cases and the 3 that are red on the base (`deploy::the_ui_is_served_from_the_binary`, `observe_panel::the_panel_is_one_self_contained_page`, `observe_panel::the_panel_may_be_framed`).
- The "before" count comes from a second run of both suites with `--skip adversary_w28_u4`: 21 + 152 + 552 = 725 executed.
- `cargo clippy ... --all-targets -D warnings` is clean, and `cargo fmt -p spindle-cli -p spindle-ingress -- --check` exits 0.

**4. Findings**

| # | file:line | Finding | What reaches it | Verdict / origin |
|---|---|---|---|---|
| 1 | `crates/spindle-cli/src/seed.rs:569` (`Seeding::cut`), called at `serve.rs:627` | In `reset_seeded`, `resetting()` publishes `running` and then `console::reset` empties the stores. After that, `cut()` (through `end()`) publishes the cut run as `ended`, and the state reads `ended` until `restart()` publishes again at `:593`. During that gap, ready answers 200 while the stores are empty. This contradicts the `spindle.observe.SeedState` contract (`ess/domains/observe.yaml`: "reads `running` from before the reset empties the stores … until it ends") and the comment at `serve.rs:613`. Fix: no publish from `cut` inside a reset with no world, or publish only after `restart`. | Any `POST /api/reset` with no body on a seeded run while something probes readiness. It reproduces 5 out of 5 times. | NEEDS-CHANGE / introduced |
| 2 | `crates/spindle-cli/src/serve.rs:537` (the `stopped` closure), drain at `:503` | The drain (default 20 s) takes a reset that was queued at SIGTERM. `reset_seeded` empties the stores and calls `restart`, which sets the marker to 0. `finish` sees that a signal arrived and sends no entry. The reset still answers 200. This breaks `story:reset-reapplies-seed` ("answers with the seed's effects in place"). On the base, this reset answered 503. The implementor kept the guarding tests green by adding `--drain 0` to `signal_exit::a_request_queued_at_the_signal_reads_503` and to `adversary_w27_u2::sigint_with_a_grant_queued_reads_503_and_exits_130`; that second file is outside U4's file assignment. Fix: during the drain, either refuse a queued reset with 503, or let its seed run to the end like any request in flight. | A reset that is pending while the orchestrator sends SIGTERM, with the default `--drain`. A later restart without `--seed` finds an empty state. | NEEDS-CHANGE / introduced |
| 3 | `crates/spindle-cli/src/serve.rs:503` | `Instant::now() + serving.drain` panics when `--drain` is above what `Instant` can add (any `u64` above about 9.2e18). The run then exits 101 instead of 143 and does not drain. Fix: `checked_add`, or cap the value in `cli.rs`. | Nobody was shown passing a value that large. | CONFIRMED / introduced, severity note |
| 4 | `crates/spindle-ingress/src/health.rs:16` (`LIVENESS` 60 s), beats only in `Listener::next` (`listener.rs:465,467`) | Two paths hold the run's thread past 60 s without calling `next`. A reset re-seeds every entry inside `reset_seeded` → `finish`. A delivered command with `on_failure: retry` makes up to `RETRY_ATTEMPTS` = 5 attempts with no backoff (`spindle-host/src/host.rs:187`), and each external call may take up to `MAX_TIMEOUT_MS` = 30 000. So a run that is still working reads 503 on `/__spindle/live`, against the `spindle.observe.Liveness` text ("a command held by one does not read as a run that stopped"). A liveness probe would then restart a working container. | A configuration that allows this exists (retry binding with a 30 s timeout, or a reset over several slow entries). No case: the CLI cannot change the 60 s bound, so a test would break the 60 s limit. | INFEASIBLE / introduced, warning |

**5. Attacked and not broken**
- The 403 guard on all three probes under `--trusted-caller-header`, with no secret and with a wrong one.
- Ready turns 503 at SIGTERM while a command is held (the signal handler sets the flag directly).
- Requests after SIGTERM answer 503, including on a kept-alive connection and for console routes.
- A body that finishes after the stop answers 503 and is never queued.
- Exit 143 on SIGTERM; exit 130 on SIGINT with the default drain.
- `--drain 0` answers none of the queued requests.
- A kept state whose marker records every entry done reads `ended` from the first request.
- The view holds the last entry once ready reads 200 (on the start seed's path).
- The new JSON fixtures match what the routes answer.

**6. Paths written outside the worktree**
- `<local path>`: the run logs (`ing1.log`, `cli1.log`, `cli2.log`, one `<case>.log` per case, `suite-ingress.log`, `suite-cli.log`, `suite-skip.log`) and `commit.txt`.

**7. Findings block**
```findings
- file: crates/spindle-cli/src/seed.rs
  line: 569
  category: concurrency
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a plain reset publishes the cut seed run as ended after console::reset emptied the stores and before Seeding::restart, so /__spindle/ready reads 200 on empty stores, against spindle.observe.SeedState
- file: crates/spindle-cli/src/serve.rs
  line: 537
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the default drain takes a reset queued at SIGTERM, empties the stores, sends no seed entry because a signal arrived, and still answers 200, where the base answered 503
- file: crates/spindle-cli/src/serve.rs
  line: 503
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a --drain above what Instant can add panics on SIGTERM with exit 101 instead of draining and exiting 143
- file: crates/spindle-ingress/src/health.rs
  line: 16
  category: judgement
  severity: warning
  verdict: INFEASIBLE
  origin: introduced
  message: liveness beats only in Listener::next, so a reset re-seeding slow entries or a retried delivery (5 attempts of up to 30 s each) holds the thread past the 60 s bound and a working run reads 503
```

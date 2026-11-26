---
format: aep.planning-md/3
id: story:test-step-within-budget
kind: story
status: active
title: The test step ends within 2 minutes and kills a test at 60 s
relations:
- serves: vision:spindle
- depends_on: story:planning-only-prs-skip-tests
scope:
- confidence: cited
  path: .config/nextest.toml
- confidence: cited
  path: .github/workflows/ci.yml
- confidence: cited
  path: Taskfile.yml
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/adversary_replay.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/concurrent_reads.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/conform.rs
- confidence: cited
  path: crates/spindle-conformance/tests/spindle-conformance/adversary_conform.rs
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host
- confidence: cited
  path: crates/spindle-host/tests/spindle-host/adversary.rs
- confidence: cited
  path: crates/spindle-ingress/tests/spindle-ingress/metrics_read.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/adversary_append.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/resting_rows.rs
- confidence: cited
  path: crates/spindle-telemetry/tests/spindle-telemetry/adversary_transport.rs
- confidence: cited
  path: crates/spindle-telemetry/tests/spindle-telemetry/transport.rs
- confidence: inferred
  path: scripts
revision: 48
transitions:
- {from: "draft", to: "proposed", at: "2026-11-22T18:23:51Z", actor: "human:timo", revision: 5}
- {from: "proposed", to: "active", at: "2026-11-26T04:51:42Z", actor: "human:operator", revision: 48, decided_on: {"recorded":{"review_outcome":19}}}
---
# The test step ends within 2 minutes and kills a test at 60 s

## Outcome

`cargo test --workspace` is the test step of `task check` and of CI's `Gate`. The beyond10x test
budget (`AGENTS.md` of the organisation, § Tests stay fast) holds the step to 120 s and every test
to 60 s, with the runner killing a test at 60 s. Measured on 2026-11-22 during wave review-017, on
a loaded build pool (host load about 100):

- the `spindle-kernel` integration binary ran 1371 s; `resting_rows::` and `adversary_append::`
  run single-threaded took 1738 s; `adversary_append::a_long_list_keeps_every_element_in_order_on_memory`
  and three `resting_rows::` tests ran over 60 s;
- the `spindle-cli` binary ran 321 s, with several `conform::` tests over 60 s; the `spindle-conformance` binary holds
  `adversary_conform::a_host_that_drops_a_view_row_fails_the_gate`, over 60 s;
- the `spindle-host` binary ran 164 s, with several tests over 60 s;
- `spindle-telemetry`'s `adversary_transport::a_collector_that_accepts_once_and_never_reads_does_not_block_a_send`
  (200,000 sends) runs over 60 s on 0.17.0 too; the binary ran 113 s on 0.17.0;
- tests with a wall-clock bound fail under load and pass alone:
  `transport::a_missing_collector_drops_frames_and_counts_the_drops` (5.04 ms against 5 ms),
  `metrics_read::a_metrics_read_and_a_command_sent_during_it_answer_within_the_budget` (`spindle-ingress`),
  `adversary_replay::a_long_scenario_replays_in_time_linear_in_its_steps`,
  `adversary::bindings_that_trigger_each_other_do_not_hang_the_host` (5 s deadline);
- `spindle-cli`'s `concurrent_reads::reads_are_answered_from_the_last_committed_write_while_a_command_is_held`
  failed in the local gate of PR 60 on a loaded pool (sqlite median 169 ms against its 100 ms
  bound) and passed on CI.

After this story the test step runs under cargo-nextest with `slow-timeout` and `terminate-after`
killing a test at 60 s, prints its ten slowest tests, and fails when it runs over 120 s, locally
and in CI; the tests above are made fast (an expensive product built once, fewer iterations with
the property kept) or their bound is a count, not a wall-clock time.

## Acceptance

- `.config/nextest.toml` kills a test at 60 s: a probe test that sleeps 61 s, run with the
  repository's profile, fails as timed out; the change's report shows the run. The probe is not
  committed.
- `scripts/within.sh <seconds> <command>` prints the command's wall time and exits non-zero when it
  runs over the bound; `task check:within_fails_over_its_bound` runs it on a 2-second sleep with a
  1-second bound and expects the failure.
- The test step of `task check` and of CI's `Gate` is `cargo nextest run --workspace` under
  `scripts/within.sh 120`, after a separate build step (`cargo nextest run --workspace --no-run`)
  whose compile time is not part of the step. On the PR's `Gate` run the test step ends within
  120 s and prints the ten slowest tests; its log shows both.
- The Rust tests named in § Outcome each run under 60 s in that `Gate` run's nextest output, and
  every other Rust test runs under 60 s because the profile kills it.
- The three tests with a sub-second bound today assert a count or a bound of at least 1 s:
  `transport::a_missing_collector_drops_frames_and_counts_the_drops` (5 ms today, `transport.rs:156`),
  `metrics_read::a_metrics_read_and_a_command_sent_during_it_answer_within_the_budget` (200 ms
  today) and
  `concurrent_reads::reads_are_answered_from_the_last_committed_write_while_a_command_is_held`
  (100 ms today, `concurrent_reads.rs:37`).

## Origin

Wave review-017 (stories from the 0.17.0 review), 2026-11-22: every implementor reported the
pre-existing slow tests above. `concurrent_reads::reads_are_answered_from_the_last_committed_write_while_a_command_is_held`
is in § Outcome because it failed the same step in PR 60's local gate on a loaded pool (a 100 ms
bound). The `adversary_sampling` race of PR 58's `Gate` (run 45015271776) was fixed by
`story:ci-tests-deterministic` (commit `1e53e0a2e`) and is not part of this story.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** gate wiring: `.config/nextest.toml` (new), `Taskfile.yml` task `check`, `.github/workflows/ci.yml` job `Gate` — cited
- **Files:** `.config/nextest.toml` (new; slow-timeout with terminate-after at 60 s) — cited
- **Files:** `Taskfile.yml`, task `check`, last command `cargo test --workspace --locked --no-fail-fast` — cited
- **Files:** `.github/workflows/ci.yml`, job `Gate` (install cargo-nextest, step budget) — cited
- **Files:** `crates/spindle-kernel/tests/spindle-kernel/adversary_append.rs:276` `a_long_list_keeps_every_element_in_order_on_memory` — cited
- **Files:** `crates/spindle-kernel/tests/spindle-kernel/resting_rows.rs` (shared fixture at :186, `cap = 20 ms` at :271) — cited
- **Files:** `crates/spindle-conformance/tests/spindle-conformance/adversary_conform.rs:257` `a_host_that_drops_a_view_row_fails_the_gate` — cited (the story places it under `spindle-cli`; the tree has it in `spindle-conformance`)
- **Files:** `crates/spindle-cli/tests/spindle-cli/conform.rs` — cited
- **Files:** `crates/spindle-cli/tests/spindle-cli/adversary_replay.rs:106` `a_long_scenario_replays_in_time_linear_in_its_steps` — cited
- **Files:** `crates/spindle-cli/tests/spindle-cli/concurrent_reads.rs:288` `reads_are_answered_from_the_last_committed_write_while_a_command_is_held` (`BUDGET` 100 ms at :37) — cited
- **Files:** `crates/spindle-host/tests/spindle-host/adversary.rs:120` `bindings_that_trigger_each_other_do_not_hang_the_host` — cited
- **Files:** `crates/spindle-telemetry/tests/spindle-telemetry/adversary_transport.rs:123` (200,000 sends at :134) — cited
- **Files:** `crates/spindle-telemetry/tests/spindle-telemetry/transport.rs:140` `a_missing_collector_drops_frames_and_counts_the_drops` (5 ms bound at :156) — cited
- **Files:** `crates/spindle-ingress/tests/spindle-ingress/metrics_read.rs:272` `a_metrics_read_and_a_command_sent_during_it_answer_within_the_budget` — cited
- **Also likely:** the unnamed `spindle-host` tests over 60 s, in `crates/spindle-host/tests/spindle-host/` — inferred
- **Also likely:** `.github/workflows/ci.yml` job `macOS` step "The in-process test lanes" — inferred
- **Confidence:** high for the gate wiring and the named tests; medium for the unnamed spindle-host tests
- **Would collide with:** any unit touching `Taskfile.yml` task `check`, `.github/workflows/ci.yml` job `Gate`, or the test files above
- **Safety:** under nextest each test runs in its own process; a `OnceLock` fixture is built once per test, so the switch can make a binary slower before it makes it faster — inferred

## Shared files

- `.github/workflows/ci.yml` (job `Gate`) and `Taskfile.yml` (task `check`): shared with
  `story:planning-only-prs-skip-tests` (built before this story) and
  `story:wasm-lanes-within-budget` (built after it, and uses `scripts/within.sh`).
- `crates/spindle-docs/src/status.rs` and the generated status pages name tests in
  `concurrent_reads.rs`, `resting_rows.rs` and `conform.rs`: this story renames none of them, so it
  leaves the status files to `story:reset-reapplies-seed`.

## Not covered

- The UI test `TopologyPanel.adversary.test.ts › a graph of 2000 commands` (vitest default 5 s,
  failing only under load): it runs in `ui`'s vitest step, not in the cargo test step this story
  bounds.

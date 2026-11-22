---
format: aep.planning-md/3
id: story:test-step-within-budget
kind: story
status: draft
title: The test step ends within 2 minutes and kills a test at 60 s
relations:
- serves: vision:spindle
revision: 3
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
- the `spindle-cli` binary ran 321 s; several `conform::` tests and
  `adversary_conform::a_host_that_drops_a_view_row_fails_the_gate` ran over 60 s;
- the `spindle-host` binary ran 164 s, with several tests over 60 s;
- `spindle-telemetry`'s `adversary_transport::a_collector_that_accepts_once_and_never_reads_does_not_block_a_send`
  (200,000 sends) runs over 60 s on 0.17.0 too; the binary ran 113 s on 0.17.0;
- tests with a wall-clock bound fail under load and pass alone:
  `transport::a_missing_collector_drops_frames_and_counts_the_drops` (5.04 ms against 5 ms),
  `metrics_read::a_metrics_read_and_a_command_sent_during_it_answer_within_the_budget`,
  `adversary_replay::a_long_scenario_replays_in_time_linear_in_its_steps`,
  `adversary::bindings_that_trigger_each_other_do_not_hang_the_host` (5 s deadline), and the UI test
  `TopologyPanel.adversary.test.ts › a graph of 2000 commands` (5 s);
- `spindle-target`'s `adversary_sampling::an_instance_that_ended_without_a_wait_is_not_sampled`
  failed once on PR 58's `Gate` (run 45015271776, `a20329e2`; the wave does not touch
  `spindle-target`): its wait loop stops on a sample with `memory_bytes == 0` and then asserts no
  sample, so a zombie read as `Some` with zero memory fails it.

After this story the test step runs under cargo-nextest with `slow-timeout` and `terminate-after`
killing a test at 60 s, prints its ten slowest tests, and fails when it runs over 120 s, locally
and in CI; the tests above are made fast (an expensive product built once, fewer iterations with
the property kept) or their bound is a count, not a wall-clock time.

## Acceptance

- `.config/nextest.toml` kills a test at 60 s; a test that sleeps 61 s fails the step.
- `task check`'s test step and CI's `Gate` test step each end within 120 s on an idle runner and
  print the ten slowest tests.
- No test in the list above runs over 60 s or depends on wall-clock timing under 1 s.

## Origin

Wave review-017 (stories from the 0.17.0 review), 2026-11-22: every implementor reported the
pre-existing slow tests above.

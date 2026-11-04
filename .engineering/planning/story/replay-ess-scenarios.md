---
format: aep.planning-md/3
id: story:replay-ess-scenarios
kind: story
status: active
title: Replay ESS scenarios
relations:
- serves: vision:spindle
- decomposes: epic:m0-run-in-process
- depends_on: story:views-with-filters-and-lists
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 3}
---
# Replay ESS scenarios

## Outcome

`b10x-spindle run <spec> --environment <env> --scenario <file>` replays an `ess-scenario/1` timeline: each step a command with the outcome it expects, `$instance` captures from emitted events, view asserts of the form "the list contains a row with these fields". Exit 0 when every step and assert holds; otherwise the first divergence is printed.

## Acceptance

- `a_scenario_whose_steps_all_hold_exits_zero`
- `a_capture_feeds_a_later_step`
- `the_first_divergent_step_is_named`

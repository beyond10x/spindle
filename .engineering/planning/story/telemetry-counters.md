---
format: aep.planning-md/3
id: story:telemetry-counters
kind: story
status: active
title: Telemetry counters
relations:
- serves: vision:spindle
- decomposes: epic:observe
- depends_on: story:execute-commands
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:52Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:52Z", actor: "human:timo", revision: 3}
---
# Telemetry counters

## Outcome

A `Telemetry` port (`crates/spindle-telemetry`) with counters and histograms in kernel, store, channel and timer; the counter catalogue is derived from the compiled IR; `b10x-spindle metrics <spec> --component <c>` prints it. Design § 2, § 3.

## Acceptance

- `every_command_outcome_and_event_of_a_component_has_a_counter`
- `executing_a_command_increments_its_outcome_counter`
- `recording_never_blocks_a_command`

---
format: aep.planning-md/3
id: story:launcher-resource-sampling
kind: story
status: implemented
title: Resource sampling by the launcher
relations:
- serves: vision:spindle
- decomposes: epic:observe
- depends_on: story:deployment-target-contract
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:52Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:52Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-04T16:05:55Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
# Resource sampling by the launcher

## Outcome

Each launcher reports host-side CPU seconds and memory per instance and its share of the requested quota. Design § 4.

## Acceptance

- `cpu_share_is_cpu_seconds_over_wall_time_and_requested_vcpus`
- `a_process_instance_without_a_limit_reports_no_quota`

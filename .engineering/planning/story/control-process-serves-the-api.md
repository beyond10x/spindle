---
format: aep.planning-md/3
id: story:control-process-serves-the-api
kind: story
status: active
title: The control process serves the API
relations:
- serves: vision:spindle
- decomposes: epic:observe
- depends_on: story:telemetry-transport
- depends_on: story:launcher-resource-sampling
- depends_on: story:observe-api-contract
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:52Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:52Z", actor: "human:timo", revision: 3}
---
# The control process serves the API

## Outcome

`b10x-spindle deploy` runs `spindle-control`: places and launches instances, collects samples, serves the read-only API on 127.0.0.1 with a token and a Host check. Design § 5.

## Acceptance

- `a_request_without_the_token_is_refused`
- `a_request_with_a_foreign_host_header_is_refused`
- `the_stream_carries_one_sample_per_instance_per_period`

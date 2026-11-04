---
format: aep.planning-md/3
id: story:rally-runs-as-three-processes
kind: story
status: draft
title: Rally runs as three processes
relations:
- decomposes: epic:m1-m3-rally-in-three-vms
revision: 1
---
# Rally runs as three processes

## Outcome

Three `b10x-spindle run` processes, one per rally component, each with its own environment
document using `channel = tcp`, play a match to a winner.

## Acceptance

- `a_match_between_three_processes_ends_with_a_winner`.
- `the_same_bundle_runs_in_one_process_with_channel_in_process` — only the environment differs.

## Scope

`examples/rally/`, `crates/spindle-cli/`.

---
format: aep.planning-md/3
id: story:component-process-observe
kind: story
status: draft
title: Every component process of a split run answers the reserved routes
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability-depth
- depends_on: story:metric-families-route
- depends_on: story:run-one-component
- depends_on: story:live-topology
scope:
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/component.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli
- confidence: cited
  path: crates/spindle-ingress/src/observe.rs
revision: 3
---
# Every component process of a split run answers the reserved routes

## Outcome

A process started by `run --component` (`story:run-one-component`) with `--observe` answers
`/__spindle/metrics`, `/__spindle/metrics/families`, `/__spindle/requests` and
`/__spindle/topology` for its own component: on its ingress where the component is reached over
the network, otherwise on an `--observe-listen` address. Its topology holds its own component's
nodes and edges. Traces across processes are `story:trace-crosses-processes`, not this story.

## Acceptance

- `each_component_process_counts_only_its_commands`: with rally split into three processes, each
  process's `spindle_commands_total` holds only commands its component accepts.
- `the_split_counts_add_up`: the three processes' `spindle_commands_total` samples sum to what one
  `run` process reports for the same match.
- `a_component_without_ingress_listens_for_observe`: a component reached in process only answers
  `/__spindle/metrics` at `--observe-listen`.
- `each_component_process_has_its_topology`: each process's `/__spindle/topology` holds the command
  nodes of its own component and none of another's.

## Scope

- `crates/spindle-cli/src/component.rs` (generalised by `story:run-one-component`),
  `crates/spindle-cli/src/cli.rs` (`--observe`, `--observe-listen` on `run --component`).
- `crates/spindle-ingress/src/observe.rs` (the reserved routes on a component's surface).
- `crates/spindle-cli/tests/`.

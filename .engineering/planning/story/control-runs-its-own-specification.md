---
format: aep.planning-md/3
id: story:control-runs-its-own-specification
kind: story
status: draft
title: The control process runs spindle's own specification
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-control/src/answer.rs
- confidence: cited
  path: crates/spindle-control/src/control.rs
- confidence: inferred
  path: crates/spindle-control/src/placement.rs
- confidence: inferred
  path: ess
revision: 3
---
# The control process runs spindle's own specification

## Outcome

The control process keeps placement state by running spindle's own specification on
`spindle-kernel`: `DeclareTarget`, `PlanInstance`, `PlaceInstance`, `LaunchInstance` and
`ReportExit` are commands it executes, and `TargetState` and `InstanceState` are views it answers
(`docs/design/observe.md` § 5). Today `crates/spindle-control/src/control.rs` holds placement state
in memory.

## Acceptance

- `control_executes_the_placement_commands` — a deployment of two instances records each
  `DeclareTarget`, `PlanInstance`, `PlaceInstance` and `LaunchInstance` in the kernel's store.
- `an_exit_is_reported_as_report_exit` — an instance that exits is `Exited` in `InstanceState`.
- `the_deployment_answer_reads_the_views` — `GET /api/deployment` answers from `TargetState` and
  `InstanceState`.

## Origin

Cut from `story:control-process-serves-the-api` (wave 6).

## Scope

- `crates/spindle-control/src/control.rs` — cited: `Control::start`, the in-memory state.
- `crates/spindle-control/src/placement.rs` — inferred.
- `crates/spindle-control/src/answer.rs` — inferred: the deployment answer.
- `ess/` — inferred: the commands and views it runs.

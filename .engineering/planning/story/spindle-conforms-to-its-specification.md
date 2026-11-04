---
format: aep.planning-md/3
id: story:spindle-conforms-to-its-specification
kind: story
status: draft
title: Spindle conforms to its own specification
relations:
- decomposes: epic:m0-run-in-process
- serves: vision:spindle
- depends_on: story:replay-ess-scenarios
- depends_on: story:spindle-admission-rules-are-declared
- depends_on: story:spindle-placement-rules-are-declared
scope:
- confidence: inferred
  path: Cargo.toml
- confidence: cited
  path: Taskfile.yml
- confidence: inferred
  path: crates/spindle-conformance
revision: 6
---
# Spindle conforms to its own specification

## Outcome

The conformance suite ESS synthesises from `ess/` (`ess conform synthesize --path ess`) runs
against Spindle's executor through an in-process target, and `task check` runs it with its counts
held. The suite covers every domain `ess/system.yaml` declares that has commands
(`spindle.deployment`, `spindle.placement`; `spindle.observe` declares the API nouns). The
`ess-conformance-report` it writes is the evidence `aep plan artifact evidence --from` records.

## Acceptance

Each is a named check with the outcome it observes.

- `every_executed_scenario_passes` — the report counts every executed scenario of the suite as
  passed.
- `every_skipped_scenario_has_a_reason` — each scenario the run skips carries a recorded reason
  in the report.
- `a_failing_scenario_fails_the_gate` — a target that answers one command wrongly (a test
  double) makes the run exit non-zero, naming the scenario.
- `the_suite_counts_are_held` — `task check` compares executed and skipped counts with committed
  ones and fails on a drop.
- `the_synthesis_refusals_are_listed` — every refusal `ess conform synthesize --path ess` reports
  at the time of the run is printed with its code, not silently absent. No refusal is named here:
  the placement story removes the one the design review saw.

## Scope

- `crates/spindle-conformance` — inferred, new: the in-process target.
- `Taskfile.yml` — cited: a `check:conformance` step in `check`.
- `Cargo.toml` — inferred: the workspace dependency on `ess-conformance`, shared with
  `story:replay-ess-scenarios`.

Shared, not typed: `crates/spindle-docs/src/status.rs` (the "Scenario replay and
self-conformance" item), `README.md`, `AGENTS.md`, `Cargo.lock`.

## Depends on

- `story:replay-ess-scenarios` — added by the plan review: both execute ESS scenario steps
  against the kernel through `ess-conformance`, and the replay story lands that dependency and the
  step executor first.
- `story:spindle-admission-rules-are-declared` and `story:spindle-placement-rules-are-declared` —
  added after the round-1 critics, 2026-11-04: they change the declarations, scenarios and
  refusals of `ess/` that `the_suite_counts_are_held` and `the_synthesis_refusals_are_listed`
  commit to, so the counts are taken after them.

## Shared with story:control-process-serves-the-api

`Taskfile.yml`: this story adds a `check:conformance` step to `check`; story:control-process-serves-the-api changes `build` and `check` to build `ui/` first. Different lines of one file: if both land in one wave, the wave coordinator resolves `Taskfile.yml` at integration, as it does `Cargo.lock`.

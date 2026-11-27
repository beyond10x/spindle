---
format: aep.planning-md/3
id: story:conform-awaits-system-steps
kind: story
status: draft
title: Conformance runs a scenario that awaits a system step
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

`conform`, `run --scenario` and `spindle_conformance::HostTarget` run a scenario that awaits a
step its system takes itself (a deadline, a tick, an event binding's command): the target answers
ESS's `Target::await_outcome` from the host's own record of that step, where it answers ESS's
default `unsupported` ("this target cannot observe, or hold, a step its system takes itself") and
every such scenario is skipped today.

## Why

Reported by a consumer against 0.22.0: 50 of the 62 skips of a suite synthesized by ESS 0.99.8 are
await scenarios. `HostTarget` does not implement `await_outcome`; the default of
`ess-conformance` (pinned 0.99.2) refuses it.

## Acceptance

- `an_awaited_system_step_is_observed`: a neutral specification whose command is taken by the
  system (a deadline or an event binding); its synthesized await scenario passes under `conform`,
  and fails when the host does not take the step.
- The skip count of the repository's own conformance suite for await scenarios is 0, and
  `check:conformance` names the count.
- The ESS pin moves to the release whose `AwaitRequest` the consumer's suite uses, if 0.99.2's
  differs.

## Not covered

- Await scenarios on a browser or Node host.

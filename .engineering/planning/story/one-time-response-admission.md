---
format: aep.planning-md/3
id: story:one-time-response-admission
kind: story
status: draft
title: A scenario with a one-time response is admitted as ESS admits it
relations:
- serves: vision:spindle
revision: 1
---
# A scenario with a one-time response is admitted as ESS admits it

## Outcome

A conformance scenario whose external cause carries a one-time response runs under
`b10x-spindle conform` and replay. Today it is refused at admission with `InvalidOneTimeResponse`
("one-time event window is not anchored to an operation"), raised by ESS's `ess-conformance`
(`one_time_response.rs`) from spindle's call in `crates/spindle-conformance/src/target.rs`.

## Acceptance

- `a_one_time_response_scenario_runs_on_the_host` — a neutral fixture with one external cause and a
  one-time response passes under the host as under ESS's interpreter.
- If ESS refuses a valid input, an issue on beyond10x/ess names the fixture, and the scenario is
  listed as a known failure until it is fixed.

## Origin

Pre-existing. Reported by a downstream consumer replaying its suite during wave 7.

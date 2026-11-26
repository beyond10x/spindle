---
format: aep.planning-md/3
id: story:scenario-act-without-outcome
kind: story
status: draft
title: A scenario act that states no outcome is reported as ESS's runner reports it
relations:
- serves: vision:spindle
revision: 1
---
# A scenario act that states no outcome is reported as ESS's runner reports it

## Outcome

`run --scenario` reports a timeline act that states no expected outcome the way ESS 0.99.1's
conformance runner judges it: where the runner fails a refused or failed act, Spindle prints it as
not holding, with the outcome observed, and exits 1. Today such an act prints as holding even when
the command was refused, so a scenario reads green over a step that did nothing.

Reported 2026-11-26 by a consumer replaying business scenarios. Inferred, not traced: what ESS's
runner asserts for an act without an expected outcome; the story first establishes it from
`ess-conformance` at the pin and states it here.

## Acceptance

- `an_act_without_an_outcome_that_is_refused_is_reported_as_ess_reports_it`: a scenario whose act
  states no outcome and whose command is refused prints the act with the observed outcome and exits
  as ESS's runner exits for the same document.
- `an_act_without_an_outcome_that_runs_holds`: the same act on a command that records still holds.
- Every act's line prints the outcome observed, whether or not the act states one.

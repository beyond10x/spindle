---
format: aep.planning-md/3
id: story:spec-design-review-findings
kind: story
status: draft
title: Close the gaps the spec design review found
revision: 2
---
## Story

As the maintainer of spindle's specifications, I want the gaps a design review found between docs/design/spindle.md and the ESS sources of spindle and rally closed, so that what the design states is declared and checked.

## Source

An ess:hardening design review (technique 8) of `ess/` and `examples/rally/ess/` against `docs/design/spindle.md`, 2026-11-04: 23 findings, 10 warning, 13 note, 0 blocker. Validation at the time: spindle 51 scenarios, 1 refusal; rally 28 scenarios, 30 refusals (ESS-SYNTH-003 x18, ESS-SYNTH-010 x12).

## Acceptance

Each finding below is fixed in the specification (ess:specifying) or a scenario (ess:testing-conformance), or recorded as an ESS limit with an issue on beyond10x/ess.

## Findings

```findings
- file: ess/domains/deployment.yaml
  line: 199
  category: spec-gap
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "[missing] spindle.md:109 'exactly one implementation bound to every required capability' is not declared; admitted takes input.bindings with no guard or invariant against requires or Environment.provides"
- file: ess/domains/deployment.yaml
  line: 211
  category: spec-gap
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "[missing] spindle.md:110 'refused, naming every capability the environment lacks' is not declared; refused is external, missing comes unchecked from input and may be empty, and DeploymentRefused carries only deployment_id"
- file: ess/domains/deployment.yaml
  line: 32
  category: spec-gap
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "[missing] spindle.md:112 preference order among an environment's implementations is not declared; Binding has no rank and DecideAdmission takes the chosen bindings from its caller"
- file: ess/domains/deployment.yaml
  line: 167
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: "[missing] spindle.md:98-100 'capabilities are derived, never authored' is only in the OfferBundle summary; requires is an Operator input that nothing ties to the bundle"
- file: ess/domains/placement.yaml
  line: 207
  category: spec-gap
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: "[missing] spindle.md:284 and 107-110 make admission gate placement; PlanInstance, PlaceInstance and LaunchInstance have no guard on the Deployment's state, so an instance of a Refused deployment can be placed and launched"
- file: ess/domains/placement.yaml
  line: 264
  category: spec-gap
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: "[missing] spindle.md:110 says a refusal names every missing capability; PlaceInstance refused folds three causes into one external branch and InstanceRefused records neither cause nor capabilities"
- file: ess/domains/placement.yaml
  line: 378
  category: spec-gap
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: "[missing] observe.md:116 /api/deployment returns targets and requests; no view publishes Target or Instance.request, which is also why synthesize refuses Target/state/Retired/refuses/RetireTarget (ESS-SYNTH-001)"
- file: ess/system.yaml
  line: 10
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "[missing] observe.md:127 promises a spindle.observe domain (Sample, Counter, Catalogue); system.yaml declares only spindle.deployment and spindle.placement"
- file: ess/domains/deployment.yaml
  line: 70
  category: spec-gap
  severity: warning
  verdict: NEEDS-CHANGE
  origin: pre-existing
  message: "[unclear] spindle.md:383-384 deploys one bundle as three environments; Deployment has one environment_id and no component, and the design does not say whether that is one deployment or three"
- file: ess/domains/placement.yaml
  line: 184
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "[spec-only] RetireTarget and the Retired state have no design statement; observe.md:106-107 lists declare, plan, place, launch, report exit"
- file: ess/domains/placement.yaml
  line: 57
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "[spec-only] Target.ceiling and the 'request exceeds its ceiling' refusal have no design statement; observe.md:89 speaks only of the request"
- file: ess/domains/placement.yaml
  line: 25
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "[spec-only] TargetKind Browser has no design statement; observe.md:75-80 names process, QEMU microvm, Firecracker, Docker, Compose, Kubernetes"
- file: ess/domains/deployment.yaml
  line: 270
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: "[spec-only] StopDeployment from Admitted and its accepting already-ended outcome have no design statement; the suite accepts a stop of an Offered deployment while the summary names only refused or stopped"
- file: ess/domains/placement.yaml
  line: 329
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "[spec-only] ReportExit already-ended accepts an exit of a Planned instance (suite scenario Instance/state/Planned/accepts/ReportExit); no design statement covers it and the summary names only refused or exited"
- file: examples/rally/ess/domains/table.yaml
  line: 219
  category: spec-gap
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "[contradicts] spindle.md:374 says each Tick moves the ball one cell; served, west-returns, east-returns, top-wall and bottom-wall change only the heading and leave the ball in place"
- file: examples/rally/ess/domains/table.yaml
  line: 176
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: "[contradicts] spindle.md:380 says every value change is a guard plus increment; scoring resets ball_x and ball_y to literals and the players copy input values"
- file: examples/rally/ess/domains/table.yaml
  line: 26
  category: spec-gap
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "[missing] the grid bounds the caller relies on (rally.rs:8-11, OutsideGrid at 95-99) are only in the domain summary; ball_x, ball_y and the paddle fields carry no invariant"
- file: examples/rally/ess/domains/table.yaml
  line: 49
  category: spec-gap
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "[missing] no invariant ties winner to state Won, yet the caller ends play on a non-empty winner (rally.rs:40-44, main.rs:282)"
- file: examples/rally/ess/domains/west.yaml
  line: 13
  category: spec-gap
  severity: note
  verdict: NEEDS-CHANGE
  origin: pre-existing
  message: "[unclear] spindle.md:374 gives the table both paddles and spindle.md:375 gives each player its paddle; the spec stores the paddle on the table and a copy on the player"
- file: examples/rally/ess/components.yaml
  line: 17
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: "[spec-only] rally-table publishes PaddleMoved, which spindle.md:378 does not list and no binding consumes"
- file: examples/rally/ess/domains/west.yaml
  line: 36
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: "[spec-only] first-sight seats the player and asks for nothing (same in east.yaml:36); spindle.md:375 has no such rule"
- file: examples/rally/ess/components.yaml
  line: 72
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: "[spec-only] all five bindings declare delivery at_most_once and on_failure drop with no design statement; the four ESS-SYNTH-010 delivery refusals follow from it"
- file: examples/rally/ess/domains/table.yaml
  line: 87
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "[spec-only] target-too-low (target_score < 1) has no design statement; only the play caller and its test rely on it"
```

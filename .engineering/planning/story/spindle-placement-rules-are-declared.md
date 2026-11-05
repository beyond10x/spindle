---
format: aep.planning-md/3
id: story:spindle-placement-rules-are-declared
kind: story
status: implemented
title: spindle.placement declares the placement rules
relations:
- serves: vision:spindle
- decomposes: story:spec-design-review-findings
- depends_on: story:spindle-admission-rules-are-declared
scope:
- confidence: inferred
  path: docs/design/observe.md
- confidence: cited
  path: ess/domains/placement.yaml
revision: 8
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T21:33:53Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"review_outcome":3}}}
- {from: "proposed", to: "active", at: "2026-11-04T21:33:53Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"review_outcome":3}}}
- {from: "active", to: "implemented", at: "2026-11-05T09:39:01Z", actor: "human:timo", revision: 8, decided_on: {"recorded":{"test_result":2,"review_outcome":3}}}
---
## Story

As the maintainer of spindle's specification, I want `spindle.placement`
(`ess/domains/placement.yaml`) and `ess/system.yaml` to declare what `docs/design/spindle.md` and
`docs/design/observe.md` say about placement, so that an instance of a refused deployment cannot be
placed in the model and the observe API's deployment answer is published by a view.

Split out of `story:spec-design-review-findings` by a plan review on 2026-11-04. It owns the eight
findings below: three warnings, five notes.

## Acceptance

A finding is closed when its check holds on `main`. Each check is satisfied by one of: a
declaration that `ess specify validate --path ess` accepts and a synthesised scenario of
`ess conform synthesize --path ess` exercises; a design statement for behaviour that is only in
the specification; or a link in this body to an issue on `https://github.com/beyond10x/ess/issues`
recording an ESS limit. Line numbers are those of the review's base and move as fixes land.

- `an_instance_of_a_refused_deployment_cannot_be_placed` — `PlanInstance`, `PlaceInstance` and
  `LaunchInstance` refuse while the deployment is not `Admitted`; a synthesised scenario shows the
  refusal (finding 5, warning).
- `a_placement_refusal_names_its_cause` — `PlaceInstance`'s refusal distinguishes ceiling,
  admission and target state, and `InstanceRefused` carries the cause (finding 6, warning).
- `targets_and_requests_are_published_by_a_view` — a view of `spindle.placement` publishes
  `Target` and `Instance.request` (finding 7, warning).
- `retire_target_is_synthesised` — `ess conform synthesize --path ess` no longer refuses
  `Target/state/Retired/refuses/RetireTarget` with `ESS-SYNTH-001` (finding 7, warning).
- `retire_target_is_stated` — the design states `RetireTarget` and `Retired`, or the
  specification drops them (finding 10, note).
- `target_ceiling_is_stated` — the design states `Target.ceiling` and the over-ceiling refusal
  (finding 11, note).
- `browser_target_kind_is_stated` — the design names `Browser` as a target kind, or the
  specification drops it (finding 12, note).
- `report_exit_of_a_planned_instance_is_stated` — the design states that an exit report for a
  `Planned` instance is accepted unchanged, or the specification refuses it (finding 14, note).

## Already holds

Finding 8 (`ess/system.yaml` does not declare `spindle.observe`) holds on `main` at `b9b0cd9`:
`domains:` lists `spindle.deployment`, `spindle.placement` and `spindle.observe`. No work closes
it, so it has no acceptance check; it stays in the findings block as the review's record. Moved
out of the acceptance after the round-1 acceptance critic, 2026-11-04.

## Scope

- `ess/domains/placement.yaml` — cited (seven findings name it).
- `docs/design/observe.md` — inferred: design statements for findings 10, 11, 12 and 14.

## Order

Runs after `story:spindle-admission-rules-are-declared` (`depends_on`): the guard in
`an_instance_of_a_refused_deployment_cannot_be_placed` and the `admission` cause read the
`Deployment` state and `DeploymentRefused` that story reshapes, and `placement.yaml` references
`spindle.deployment` types.

## Findings

```findings
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
- file: ess/domains/placement.yaml
  line: 329
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "[spec-only] ReportExit already-ended accepts an exit of a Planned instance (suite scenario Instance/state/Planned/accepts/ReportExit); no design statement covers it and the summary names only refused or exited"
```

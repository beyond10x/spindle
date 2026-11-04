---
format: aep.planning-md/3
id: story:spindle-admission-rules-are-declared
kind: story
status: active
title: spindle.deployment declares the admission rules
relations:
- serves: vision:spindle
- decomposes: story:spec-design-review-findings
scope:
- confidence: inferred
  path: docs/design/spindle.md
- confidence: cited
  path: ess/domains/deployment.yaml
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T21:33:53Z", actor: "human:timo", revision: 5, decided_on: {"recorded":{"review_outcome":2}}}
- {from: "proposed", to: "active", at: "2026-11-04T21:33:53Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"review_outcome":2}}}
---
## Story

As the maintainer of spindle's specification, I want `spindle.deployment`
(`ess/domains/deployment.yaml`) to declare the admission rules `docs/design/spindle.md` § 3a
states, so that the conformance suite checks what admission promises rather than taking the
caller's word for it.

Split out of `story:spec-design-review-findings` by a plan review on 2026-11-04. It owns the six
findings below: four warnings, two notes.

## Acceptance

A finding is closed when its check holds on `main`. Each check is satisfied by one of: a
declaration that `ess specify validate --path ess` accepts and a synthesised scenario of
`ess conform synthesize --path ess` exercises; a design statement in `docs/design/spindle.md` for
behaviour that is only in the specification; or a link in this body to an issue on
`https://github.com/beyond10x/ess/issues` recording an ESS limit. Line numbers are those of the
review's base and move as fixes land; the checks name the rule, not the line.

- `admitted_binds_exactly_one_implementation_per_requirement` — a `DecideAdmission` input whose
  bindings miss a required capability, or bind one twice, is refused by a guard or an invariant
  (finding 1, warning).
- `deployment_refused_carries_the_missing_capabilities` — a synthesised scenario of the `refused`
  outcome shows `DeploymentRefused` with the missing capabilities in its payload (finding 2,
  warning).
- `a_refusal_naming_no_capability_is_refused` — `DecideAdmission` with outcome `refused` and an
  empty `missing` list is refused by a guard or an invariant (finding 2, warning).
- `preference_order_is_declared` — a binding's rank among the environment's implementations is a
  field the specification holds, or the design records that preference stays a runtime rule
  outside the model (finding 3, warning).
- `requires_is_tied_to_the_bundle` — `requires` on `OfferBundle` is declared as derived from the
  bundle, or the design records why it is an operator input (finding 4, note).
- `one_bundle_in_three_environments_is_decided` — the design states whether rally in three
  microVMs is one deployment or three, and `Deployment` matches it (finding 9, warning, verdict
  NEEDS-CHANGE: a design decision first).
- `stop_from_admitted_and_offered_is_stated` — the design states `StopDeployment` from `Admitted`
  and from `Offered`, or the specification refuses them (finding 13, note).

## Scope

- `ess/domains/deployment.yaml` — cited (every finding names it).
- `docs/design/spindle.md` — inferred: the design statements for findings 4, 9 and 13.

## Order

Two stories read what this one changes, and both run after it (`depends_on`, added after the
round-1 critics, 2026-11-04):

- `story:spindle-placement-rules-are-declared` guards placement on the `Deployment` state and
  `DeploymentRefused` that this story reshapes; `placement.yaml` references `spindle.deployment`
  types, so one `ess specify validate --path ess` covers both.
- `story:rally-rules-are-declared` edits `docs/design/spindle.md` § 13, where finding 9's
  statement (`spindle.md:383-384`, one deployment or three for rally) also lands.

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
- file: ess/domains/deployment.yaml
  line: 70
  category: spec-gap
  severity: warning
  verdict: NEEDS-CHANGE
  origin: pre-existing
  message: "[unclear] spindle.md:383-384 deploys one bundle as three environments; Deployment has one environment_id and no component, and the design does not say whether that is one deployment or three"
- file: ess/domains/deployment.yaml
  line: 270
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: "[spec-only] StopDeployment from Admitted and its accepting already-ended outcome have no design statement; the suite accepts a stop of an Offered deployment while the summary names only refused or stopped"
```

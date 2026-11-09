---
format: aep.planning-md/3
id: story:admission-decision-checked-by-model
kind: story
status: active
title: The admission decision is checked by the model, not by its caller
relations:
- decomposes: epic:spec-states-runtime
- informed_by: review-result:ess-review-2026-11-09
- serves: vision:spindle
scope:
- confidence: inferred
  path: conformance-known-failures.json
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/run.rs
- confidence: inferred
  path: crates/spindle-control/src/control.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/deployment.rs
- confidence: cited
  path: ess/domains/deployment.yaml
revision: 8
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T21:33:08Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T21:33:08Z", actor: "human:timo", revision: 3}
---
# The admission decision is checked by the model, not by its caller

## Outcome

`spindle.deployment` decides what it can decide from its own rows, and says in the model what it
leaves to the caller. Findings from `review-result:ess-review-2026-11-09`:

- H4 (V): `OfferBundle` takes `requires` as input (`ess/domains/deployment.yaml:173-174`) and
  stores it (`:189`), while the outcome's summary (`:190-191`) and `AGENTS.md` ("Capabilities are
  derived, never authored") say it is derived.
- H5 (V): `admitted` (`:212-224`) never reads `Environment.provides` (`:52`), so a binding to an
  implementation the environment does not offer is admitted.
- H6 (R): `missing` is `DecideAdmission` input (`:202-203`) and is stored on the row (`:76-77`).
- H7 (R): `refused` (`:238-259`) admits a decision that `AdmissionDecisionInvalid` (`:128-130`)
  calls invalid, because it checks neither that the bindings name only required capabilities nor
  that `missing` plus the bindings cover `requires`.
- H15 (R): synthesis refuses 4 scenarios (ESS-SYNTH-003,
  `Deployment/state/{Admitted,Refused,Running,Stopped}/refuses/DecideAdmission`): `wrong-state`
  (`:261-267`) and `not-decided` (`:269-274`) state validity in prose only.
- H16 (R): the view `DeploymentState` (`:360-371`) does not publish `bundle`, `environment_id` or
  `requires`, which produces 8 synthesis notes ("a refusal that changed it is not checked").

Where ESS 0.90 cannot express a check (for example deriving `requires` inside the model), the
specification says who supplies the value and why, in the actor's and the outcome's summary, and
the story's report cites the ESS refusal or reference that shows it.

## Acceptance

- `ess specify validate --path ess` exits 0.
- `ess verify conform synthesize --path ess` reports 0 ESS-SYNTH-003 refusals for
  `DecideAdmission`. If any remain, each is named in the report with the construct ESS lacks.
- None of the 8 notes about `bundle`, `environment_id` or `requires` that no view publishes is
  printed.
- A decision binding a capability the environment's `provides` does not hold is not answered
  `admitted`. This is shown by a synthesised scenario, or by a kernel test that runs `ess/` through
  `spindle-kernel`.
- A decision that is invalid by `AdmissionDecisionInvalid`'s own text is not answered `refused`.
- `requires` and `missing` are either computed by the model or documented as the admitter's
  answer. Neither is written by the bundle's author.
- Every test in `crates/spindle-kernel` and `crates/spindle-cli` that sends the changed inputs
  passes, and `conformance-known-failures.json` is left for the coordinator to re-pin.

## Out of scope

`Environment` and `RegisterEnvironment` (`deployment.yaml:40-59`, `:140-162`) belong to
`story:environment-implementations-specified`.

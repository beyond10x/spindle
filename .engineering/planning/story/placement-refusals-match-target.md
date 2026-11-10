---
format: aep.planning-md/3
id: story:placement-refusals-match-target
kind: story
status: implemented
title: Placement refusals are the ones the target code returns, decided by the model
relations:
- decomposes: epic:spec-states-runtime
- informed_by: review-result:ess-review-2026-11-09
- serves: vision:spindle
scope:
- confidence: inferred
  path: conformance-known-failures.json
- confidence: cited
  path: crates/spindle-target/src/placement.rs
- confidence: cited
  path: ess/domains/placement.yaml
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T21:33:08Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T21:33:09Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-10T06:35:50Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"test_result":1,"review_outcome":13,"verification":1}}}
---
# Placement refusals are the ones the target code returns, decided by the model

## Outcome

`spindle.placement` names every refusal `spindle-target` returns, and decides the ones its own rows
can decide. Findings from `review-result:ess-review-2026-11-09`:

- H1 (V): `RefusalCause` (`ess/domains/placement.yaml:36-38`) has `TargetRetired`, `NotAdmitted`
  and `AboveCeiling`. `crates/spindle-target/src/placement.rs:63` also returns
  `EnvironmentMismatch { target, admitted }`, which the specification cannot name. The
  `not-admitted` text (`placement.yaml:303-305`) folds that case in.
- H8 (R): `PlaceInstance`'s `placed` (`:276-287`) has no predicate. `target-retired` (`:289`) and
  `above-ceiling` (`:321`) are `external:` although `Target.state`, `Target.ceiling` (`:61-62`) and
  `Instance.request` (`:95-96`) are rows of this model.
- H9 (R): `LaunchInstance`'s `not-admitted` (`:349-359`) states the deployment's state as
  `external:` text. `Instance` already has a `deployment` relation (`:100-104`). This outcome is the
  one entry in `conformance-known-failures.json`.

## Acceptance

- `ess specify validate --path ess` exits 0.
- An environment mismatch has its own refusal, cause and wording. Whether one placement can carry
  several causes, as `Placement::Refused` returns (`placement.rs:125-128`), is answered in the
  specification.
- `target-retired` and `above-ceiling` are guards over model values, or the report cites what ESS
  0.90 lacks for each.
- `LaunchInstance`'s `not-admitted` decides the deployment's state through the `deployment`
  relation (`when_related:` or the ess/34 form), or the report cites why it cannot.
- `ess verify conform synthesize --path ess` writes a scenario for each outcome the specification
  now decides. The count before and after is in the report.
- Rust in `crates/spindle-target` changes only where the specification and the code must agree on a
  name. `conformance-known-failures.json` is left for the coordinator to re-pin.

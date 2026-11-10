---
format: aep.planning-md/3
id: epic:spec-states-runtime
kind: epic
status: implemented
title: Spindle's own specification says what Spindle does
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T21:33:08Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T21:33:08Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-10T06:36:11Z", actor: "human:timo", revision: 4}
---
# Spindle's own specification says what Spindle does

## Outcome

`ess/` (`spindle.deployment`, `spindle.placement`, `spindle.observe`) states the runtime's
behaviour where the code decides it, and the synthesised suite can check it. Today it validates
(`ess specify validate --path ess`: `spindle v1 — 5 file(s), valid`, ess 0.90.0, 2026-11-09) and
synthesis writes 56 scenarios with 4 refusals and 38 notes. A review of 2026-11-09 found 16
high-severity gaps between the specification, the code and the docs.

Source: `review-result:ess-review-2026-11-09`, five read-only reviewers with one lens each
(deployment and placement against the code, observe against the ingress, modelling, coverage,
testability). Each finding there carries `file:line`; a finding marked `V` was re-read by the
coordinator, `R` is the reviewer's own verification, `I` is inferred.

## Stories

- `story:admission-decision-checked-by-model`: H4, H5, H6, H7, H15, H16 (`spindle.deployment`).
- `story:placement-refusals-match-target`: H1, H8, H9 (`spindle.placement`).
- `story:unknown-component-refused-at-deploy`: H2 (`crates/spindle-control`).
- `story:environment-implementations-specified`: H10.
- `story:store-reset-and-world-specified`: H11.
- `story:periodic-timing-specified`: H12.
- `story:binding-delivery-specified`: H13.
- `story:ingress-caller-identity-specified`: H14.

H3 (`GET /__spindle/topology` is declared at `ess/domains/observe.yaml:584-588` and answered `404`
at `crates/spindle-ingress/src/observe.rs:64,673`) is the outcome of the accepted
`story:live-topology` and gets no second story.

## Out of scope

The medium and low findings of the same review. They stay in the review record.

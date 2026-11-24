---
format: aep.planning-md/3
id: story:spindle-spec-design-review-2026-11-23
kind: story
status: draft
title: Every design-review finding of 2026-11-23 is decided
revision: 1
---
# Decide the disagreements a design review found between the design and spindle's specification

## Outcome

Each finding below is decided: the design is corrected, the specification declares the rule, or
the finding is recorded as accepted in `docs/design/spindle.md` § 11. Each one needs a decision by
the design's owner, so the hardening round of 2026-11-23 did not change either side for them.

## Source

ESS hardening round 2026-11-23 (`docs/hardening/2026-11-23.md`, technique 8): one review agent,
both design documents against `ess/`, 30 findings, of which one was the planted defect. The round
fixed findings 2, 12 and 16 to 18 and filed 19 and 20 as `story:spindle-spec-cites-source-by-symbol`.
The rest, by number in the record:

| # | kind | design | specification | decision needed |
|---|---|---|---|---|
| 3 | contradicts | `observe.md` § 10: a record's `reason` "is the answer's `refused` text" | `RequestRecord.reason`: every caller value masked, at most 512 bytes | the code masks; correct the design or the code |
| 10 | contradicts | `observe.md` § 4: CPU % of quota from Δ CPU seconds over Δ wall seconds | `ResourceReading.cpu_share`: CPU seconds over wall seconds since launch; `spindle_target::ResourceSample::against` agrees | lifetime average or a windowed share |
| 8 | missing | `observe.md` § 5: `GET /api/deployment` answers targets | `spindle.observe.Deployment` has no targets; the control process answers none | add targets to the answer, or drop them from the design |
| 13, 14 | missing | `observe.md` § 10: a `contains` edge carries no traffic; `reason` is absent for `served` | no invariant ties `traffic` to `kind`, nor `reason` to `class` | declare them if ESS can express a cross-field rule, else record them as runtime rules |
| 15 | missing | `spindle.md` § 11: console routes behind a trusted proxy answer `403` without its secret | `spindle.console.ResetRefusal` names `403` only for `play --component` | verify the route and state it |
| 4 to 7 | missing | `spindle.md` § 7: digest, format, verify, diff, cut-over and rollback refusals; `[http.<name>]` tables | not modelled | planned milestones; record them as such |
| 21 to 26 | spec-only | no design statement | the `Operator` actor, `DeploymentState` and `EnvironmentState`, `spindle-control` owning `spindle.observe`, "two instances differ only in their settings", `GET /api/served`, a world's rows and mint | state each in the design or remove it |
| 27 to 30 | unclear | "substrate backend", "the operator retires a target", what readers see after a failed reset, which payload a recent answer keeps | the nearest declarations | the design's author answers the question |
| G1 | spec-only | `observe.md` § 5a: a target names the environment it provides | `DeclareTarget` refuses no environment nobody declared, while `OfferBundle` does (`unknown-environment`) | refuse it at declaration, or state that placement refuses it (`EnvironmentMismatch`) |
| G2 | unclear | no design statement on a repeated declaration | a `creates:` for an identity a row already holds (a declaring command sent twice, found by the walk) has no declared outcome; the kernel answers none and changes nothing, ESS's interpreter refuses the request | declare an `already-declared` refusal with an error, or state that a repeat is not part of the contract |

## Acceptance

- `every_design_review_finding_of_2026_11_23_is_decided`: each row above is answered in the design
  or the specification, and the record `docs/hardening/2026-11-23.md` links the change that
  answered it.

## Scope

- `docs/design/spindle.md`, `docs/design/observe.md`, `ess/domains/*.yaml`, `components.yaml`.

---
format: aep.planning-md/3
id: story:unknown-instance-unsourced-unserved
kind: story
status: draft
title: An unknown-instance refusal with unsourced fields is not served
relations:
- serves: vision:spindle
revision: 1
---
# An unknown-instance refusal with unsourced fields is not served

## Outcome

A command whose explicit `unknown_instance:` branch answers an error with a field the specification
gives no source is treated as ESS 0.87.0's served application treats it: it does not serve that
answer (`ess-synth` `determined.rs`, "the fields of error ... which the specification gives no
source"). Today spindle answers such a refusal with the field left out, a body the published schema
refuses (`ess-gen` `openapi.rs:2519-2575` requires the full payload on that branch).

## Acceptance

- `an_unknown_instance_refusal_with_an_unsourced_field_is_not_served`: the route (or, with
  `story:outcome-scoped-refusal`, that outcome) answers 501 naming the field; nothing is written.
- `an_unknown_instance_refusal_whose_fields_all_have_sources_is_served`: answers the full payload.

## Origin

Correction round of `story:refusal-payload-complete`, wave 10, 2026-11-06.

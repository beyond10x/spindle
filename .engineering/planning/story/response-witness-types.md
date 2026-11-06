---
format: aep.planning-md/3
id: story:response-witness-types
kind: story
status: draft
title: Response fields ESS answers through its witness are answered
relations:
- serves: vision:spindle
- depends_on: story:structured-response-values
revision: 1
---
# Response fields ESS answers through its witness are answered

## Outcome

A returned response field of a type ESS 0.87.0's interpreter answers through `witness::fields`
before `response::base` is answered by spindle too, instead of refused by name. After
`story:structured-response-values` these are still refused:

- records with invariants;
- maps keyed by anything but `String` or `Uuid` (for example `Map<Integer, _>`);
- String newtypes no literal fits, which ESS answers through `text_choice`;
- `Decimal`, `Duration`, `Binary64` and `Json` outside a literal newtype;
- one-time fields that are not text;
- `{generated: true}` enums and constrained newtypes.

Records that hold themselves stay refused (ESS refuses them too).

## Acceptance

- `each_witness_answered_response_type_is_answered`: one case per item above; every answer passes
  ESS's `Observation::of` and the published OpenAPI schema.
- `derivation_names_the_capability_each_answer_mints`.

## Origin

Adversary and correction rounds of `story:structured-response-values`, wave 10, 2026-11-06. If
`witness::fields` becomes public in ESS, call it instead of porting.

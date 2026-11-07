---
format: aep.planning-md/3
id: story:command-input-decimal-text
kind: story
status: draft
title: A command input Decimal is read in the spelling ESS publishes
tags:
- from-finding
relations:
- informed_by: review-result:w12-u2-adversary-1
- serves: vision:spindle
revision: 1
---
# A command input Decimal is read in the spelling ESS publishes

## Outcome

ESS publishes `Decimal` as a decimal string (`ess-gen` `types.rs:661`, ESS 0.88.0). Spindle's
command input check admits a `Decimal` only as a JSON number
(`crates/spindle-kernel/src/typed.rs:215`), so a command whose input carries `"amount": "1.50"`
answers 400 `"1.50" is not a Decimal`, though the published schema admits it.

## Acceptance

- `a_decimal_input_in_its_published_spelling_is_admitted`: a command input `"1.50"` for a `Decimal`
  field is admitted and stored as the value `1.50`; a JSON number still is.
- `a_decimal_input_that_is_not_a_decimal_is_refused_400`: `"1.5x"` answers 400 naming the field.

## Origin

`review-result:w12-u2-adversary-1`, finding on `crates/spindle-kernel/src/typed.rs:215`, decided
pre-existing by the coordinator (the code is unchanged by wave 12).

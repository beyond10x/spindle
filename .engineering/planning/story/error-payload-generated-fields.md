---
format: aep.planning-md/3
id: story:error-payload-generated-fields
kind: story
status: draft
title: A generated field in an error payload is minted as ESS mints it
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/tests/generated.rs
revision: 3
---
# A generated field in an error payload is minted as ESS mints it

## Outcome

The kernel mints `{generated: true}` in a declared error's payload (`a_generated_error_field_is_minted`, wave 3). ESS 0.86.0's interpreter mints none there (`declared_error`), so it draws no tick. Under a mint that answers ESS's tick UUIDs, every draw after such a refusal runs one tick ahead of ESS. Found while fixing the draw order for generated Booleans (wave 5). Pre-existing.

## Acceptance

- `an_error_payload_draws_as_ess_draws` — a command refused with a declared error whose payload marks a field generated, followed by a creation that mints: the minted values equal ESS's interpreter for the same sequence under the tick mint.

## Scope

- `crates/spindle-kernel/src/execute.rs` — inferred: the error payload path.
- `crates/spindle-kernel/tests/generated.rs` — cited: `a_generated_error_field_is_minted`.

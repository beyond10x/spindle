---
format: aep.planning-md/3
id: story:generated-boolean-values
kind: story
status: draft
title: Generated Boolean values
relations:
- serves: vision:spindle
- decomposes: epic:m0-run-in-process
scope:
- confidence: cited
  path: crates/spindle-capability/src/mint.rs
- confidence: cited
  path: crates/spindle-kernel/tests/generated.rs
revision: 2
---
# Generated Boolean values

## Outcome

`{generated: true}` on a `Boolean` field is minted the way ESS 0.86.0's interpreter mints it, and
the kernel no longer refuses it. Wave 3 (`story:generated-field-values`) mints Uuid, String and its
newtypes, Integer and Timestamp only; the rule lives in `crates/spindle-capability/src/mint.rs`
(`minted`), shared by the kernel and derivation.

Requested by a downstream specification probe (2026-11-05): five commands are refused on it.
Reproduced with a neutral fixture.

## Acceptance

- `a_generated_boolean_is_minted_as_ess_mints_it` — the stored and published value matches what
  ESS's interpreter produces for the same model (cited in the test).
- `derivation_names_the_capability_a_generated_boolean_needs` — or none, if ESS's mint needs none,
  stated in the test.

## Scope

- `crates/spindle-capability/src/mint.rs` — cited: `minted`.
- `crates/spindle-kernel/tests/generated.rs` — cited.

---
format: aep.planning-md/3
id: story:create-on-held-identity-unavailable
kind: story
status: draft
title: A create on a held identity is refused as unavailable
relations:
- serves: vision:spindle
revision: 1
---
# A create on a held identity is refused as unavailable

## Outcome

A creating command whose identity is already held (by an establish step or an earlier create)
answers as ESS's interpreter answers it: unavailable, because creation never replaces a row. Today
the kernel answers `Undeclared` (`crates/spindle-kernel/src/execute.rs`), which the conformance
target maps to an undeclared result (`crates/spindle-conformance/src/target.rs`).

## Acceptance

- `adversary_establish.rs::a_create_on_an_established_identity_answers_as_the_interpreter` runs
  (its `#[ignore]` is removed) and passes.
- `a_second_create_on_one_identity_answers_as_the_interpreter` — two creates, no establish step.

## Origin

Pre-existing. Found by the adversary of `story:scenario-establish-steps` in wave 7.

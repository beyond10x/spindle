---
format: aep.planning-md/3
id: story:no-value-answered-as-ess
kind: story
status: draft
title: A value the kernel cannot determine is answered as ESS answers it
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-conformance/src/target.rs
- confidence: cited
  path: crates/spindle-conformance/tests/spindle-conformance/adversary_w29_u2.rs
revision: 2
---
# A value the kernel cannot determine is answered as ESS answers it

## Outcome

When the kernel finds no value for a field an outcome needs (`no_value`: a related row nobody holds,
read through one hop or a chain), the conformance target answers the command as ESS 0.99.2's
interpreter does: `unsupported`, which ESS counts as a capability gap
(`ess-conformance` `interpret/execute.rs:355`, "to the implementation"). Today
`crates/spindle-conformance/src/target.rs:384` answers `unavailable`.

## Acceptance

- `a_one_hop_read_of_an_absent_row_is_answered_as_ess_answers_it` and
  `a_chain_whose_far_row_is_absent_is_answered_as_ess_answers_it`
  (`crates/spindle-conformance/tests/spindle-conformance/adversary_w29_u2.rs`) run un-ignored and
  pass.
- A served or stdin run keeps its own answer to the same command; only the conformance target's
  mapping changes.

## Origin

Pre-existing; red on the base 97dedd3b. Found by the adversary of wave 29 U2.

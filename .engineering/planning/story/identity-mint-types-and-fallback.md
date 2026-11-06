---
format: aep.planning-md/3
id: story:identity-mint-types-and-fallback
kind: story
status: draft
title: The host mints every identity type ESS mints, and reads value-source fallbacks
relations:
- serves: vision:spindle
revision: 1
---
# The host mints every identity type ESS mints, and reads value-source fallbacks

## Outcome

A command whose created identity has a type the host does not mint today, or whose input uses a
value source with a fallback, runs as under ESS's interpreter. Today the host answers it as
unsupported (`crates/spindle-kernel/src/mint.rs`, `crates/spindle-kernel/src/values.rs`).

## Acceptance

- `every_identity_type_ess_mints_is_minted_by_the_host` — one neutral fixture per identity type ESS
  mints gives the interpreter's identity.
- `a_value_source_fallback_answers_as_the_interpreter` — with and without the primary source.

## Origin

Pre-existing. Reported by a consumer serving a test backend (2 routes on the mint type, 1 on the
fallback); to be reproduced with neutral fixtures.

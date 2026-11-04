---
format: aep.planning-md/3
id: story:mint-creation-identities
kind: story
status: active
title: Mint creation identities
relations:
- serves: vision:spindle
- decomposes: epic:m0-run-in-process
- depends_on: story:execute-commands
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 3}
---
# Mint creation identities

## Outcome

A `creates:` outcome whose instance identity is neither an input nor a payload source runs: the runtime mints the identity (a UUIDv4 for a `Uuid` identity, through a derived `identity.mint` capability the environment binds), writes it into the new instance and into the event payload field of the same name. Today the kernel refuses it as Unsupported (`crates/spindle-kernel/src/execute.rs:351`).

## Acceptance

- `a_creation_without_an_id_input_mints_one_and_emits_it`
- `minted_identities_are_unique_across_creations`
- `an_environment_without_identity_mint_refuses_such_a_specification`

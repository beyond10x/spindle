---
format: aep.planning-md/3
id: story:mint-string-newtype-identities
kind: story
status: draft
title: Mint String-newtype identities
relations:
- serves: vision:spindle
- decomposes: epic:m0-run-in-process
- depends_on: story:generated-field-values
revision: 1
---
# Mint String-newtype identities

## Outcome

A creation whose identity is a `String` newtype, determined by neither the input nor the outcome,
is minted by the runtime: a value that satisfies the newtype's `prefix:`, `alphabet:` and length
invariants as the IR resolves them. Today `crates/spindle-kernel/src/supported.rs` refuses every
identity that is not `Uuid`.

Requested by a downstream specification probe (2026-11-04): one command is refused on this
construct. Reproduced with a neutral fixture.

## Acceptance

- `a_string_newtype_identity_is_minted` — the created instance's identity has the newtype's
  prefix and alphabet.
- `two_creations_mint_two_identities`.
- `a_newtype_the_runtime_cannot_satisfy_is_refused_by_name` — a constraint the minter cannot
  meet stays `unsupported`, naming the type.

## Scope

- `crates/spindle-kernel/src/mint.rs` — cited: `is_uuid` and the UUIDv4 mint.
- `crates/spindle-kernel/src/supported.rs` — cited: the non-`Uuid` refusal.
- `crates/spindle-kernel/tests/mint.rs` — cited.

---
format: aep.planning-md/3
id: story:mint-string-newtype-identities
kind: story
status: active
title: Mint String-newtype identities
relations:
- serves: vision:spindle
- decomposes: epic:m0-run-in-process
- depends_on: story:generated-field-values
scope:
- confidence: cited
  path: crates/spindle-kernel/src/mint.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: cited
  path: crates/spindle-kernel/tests/mint.rs
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T21:50:12Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T21:50:13Z", actor: "human:timo", revision: 3}
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

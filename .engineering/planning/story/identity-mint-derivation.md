---
format: aep.planning-md/3
id: story:identity-mint-derivation
kind: story
status: draft
title: Admission requires identity.mint only for identities the kernel mints
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-capability/src/derive.rs
- confidence: cited
  path: crates/spindle-capability/src/mint.rs
revision: 3
---
# Admission requires identity.mint only for identities the kernel mints

## Outcome

Derivation names `identity.mint` for every creation whose identity the runtime must mint, whatever its type (`crates/spindle-capability/src/derive.rs:161`). The kernel refuses some of those identity types by name (a bare `String`, a newtype whose invariants no minted text meets). Admission therefore admits commands the kernel never runs. Found by the adversary on `story:mint-string-newtype-identities` (wave 5). Pre-existing for a bare `String` identity.

## Acceptance

- `identity_mint_is_required_only_by_creations_whose_identity_the_kernel_mints` — the adversary's test, kept outside the repository at the end of wave 5, goes green: only creations whose identity `spindle_capability::mint::minted` admits require `identity.mint`; the others are reported as unsupported at admission, naming the type.

## Scope

- `crates/spindle-capability/src/derive.rs` — cited: line 161.
- `crates/spindle-capability/src/mint.rs` — cited: `minted`.

---
format: aep.planning-md/3
id: story:identity-mint-types-and-fallback
kind: story
status: implemented
title: The host mints a bare String or Integer identity, and reads value-source fallbacks
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-capability/src/mint.rs
- confidence: cited
  path: crates/spindle-kernel/src/mint.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: cited
  path: crates/spindle-kernel/src/values.rs
- confidence: cited
  path: crates/spindle-kernel/tests/fixtures/mint/domains/label.yaml
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/main.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/mint.rs
- confidence: cited
  path: website/docs/concepts/specification.md
revision: 25
transitions:
- {from: "draft", to: "proposed", at: "2026-11-22T18:23:52Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-26T04:51:43Z", actor: "human:operator", revision: 24, decided_on: {"recorded":{"review_outcome":19}}}
- {from: "active", to: "implemented", at: "2026-11-26T08:12:34Z", actor: "human:operator", revision: 25, decided_on: {"recorded":{"test_result":1,"review_outcome":21}}}
---
# The host mints a bare String or Integer identity, and reads value-source fallbacks

## Outcome

A command whose created identity is a bare `String`, an `Integer`, or an invariant-free newtype of
`Integer`, mints that identity, where the host refuses it at start today
(`crates/spindle-kernel/src/supported.rs:88`). A value source with a fallback (an `else:` literal,
`{input: other}`, `{generated: true}`) answers as ESS 0.99.1's interpreter answers it, where the
host refuses it today (`crates/spindle-kernel/src/values.rs:389`, `supported.rs:742`).

The path is the kernel's own `mint::identity` (`crates/spindle-kernel/src/mint.rs:109`):
`spindle_capability::mint::minted` already mints these types for generated fields, and `identity`
stops filtering them out. A bare `String` identity is spelled as a minted text with no prefix.
`minted` is unchanged, so generated fields mint what they mint today.

## Acceptance

- `an_identity_of_each_minted_type_is_minted`: one neutral fixture per type (a bare `String`, an
  `Integer`, an invariant-free newtype of `Integer`) is admitted at start, and its creating command
  answers its outcome with an identity of the identity's type; two creates mint two different
  identities.
- A `Uuid` identity and a `String` newtype identity mint as today: the existing tests in
  `crates/spindle-kernel/tests/spindle-kernel/mint.rs` pass, apart from the one that asserts the
  refusal of a bare `String` identity (`:185`, fixture `mint/domains/label.yaml`), which now
  asserts the mint.
- `a_value_source_fallback_answers_as_the_interpreter`: for each of the three fallback forms, a
  request with the primary input and one without answer the field value the interpreter answers;
  for `{generated: true}` the value has the field's type and the interpreter's presence.
- `outcome_scoped_refusal.rs` and `adversary_outcome_scoped_refusal.rs` in `spindle-cli` pass
  unchanged.

## Origin

Pre-existing. Reported by a consumer serving a test backend (2 routes on the mint type, 1 on the
fallback); to be reproduced with neutral fixtures.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel` — cited
- **Files:** `crates/spindle-kernel/src/mint.rs:109` (`identity`) — cited
- **Files:** `crates/spindle-kernel/src/values.rs:309` (`Sources::value`, the `InputOrGenerated { otherwise: None }` arm; `otherwise: Some(_)` falls to the refusal at `:389`) — cited
- **Files:** `crates/spindle-kernel/src/supported.rs:88` (identity refusal in `takes`) and `:706`/`:742` (value-source refusal in `value`) — cited
- **Files:** `crates/spindle-kernel/tests/spindle-kernel/mint.rs:185`, `:288` and `crates/spindle-kernel/tests/fixtures/mint/domains/label.yaml`, which assert today's refusal of a `String` identity and of unsatisfiable newtypes — cited
- **Symbols:** `mint::identity`, `mint::identity_bound`, `ResolvedPayloadValue::InputOrGenerated { otherwise }`, `ResolvedFallback::{Literal, Input}` — cited
- **Also likely:** `crates/spindle-capability/src/mint.rs:75` (`minted`, `Minted`), which decides every minted shape — inferred, needed only if identity minting is widened through the shared rule
- **Also likely:** `crates/spindle-kernel/src/execute.rs:1903` (the identity mint in the creation path) — inferred, changes only if `identity` stops returning a `Minted`
- **Also likely:** `crates/spindle-capability/src/derive.rs:409` — inferred, only if a new identity type needs a capability other than `identity.mint`
- **Also likely:** `crates/spindle-kernel/tests/spindle-kernel/main.rs` (new test module) and a new fixture directory under `crates/spindle-kernel/tests/fixtures/` — inferred
- **Documents:** `website/docs/concepts/specification.md:100` ("Any other identity type is refused") — cited; `CHANGELOG.md` — inferred
- **ESS 0.99.1:** `interpret/values.rs:60` `mint` mints every non-optional type with a witness (a `Uuid` as `00000000-0000-4000-8000-` plus the tick, other types by the synthesis witness); a `String` newtype is its prefix plus `generated-` and the tick (`witness.rs:8022`, `:1286`), where the host spells prefix plus UUID text (`mint.rs:375`); `interpret/execute.rs:2722` reads an `else:` literal, an `{input: other}` and `{generated: true}` fallback — cited
- **Confidence:** high — both refusal sites read in the tree
- **Would collide with:** any unit touching `spindle-kernel`'s `supported.rs`, `values.rs`, `mint.rs` or the creation path of `execute.rs`; `spindle-capability`'s `mint.rs`; the kernel test binary's `main.rs`
- **Safety:** `spindle_capability::mint::minted` is shared by identity minting, generated fields, derivation and error fields; widening it (for example to `Decimal`) makes `{generated: true}` fields of that type mint too, and flips `crates/spindle-cli/tests/spindle-cli/outcome_scoped_refusal.rs:7` and `adversary_outcome_scoped_refusal.rs:7` — inferred

## Not covered

- A `Bytes` or `Timestamp` identity: `minted` mints a one-byte `Bytes`
  (`crates/spindle-kernel/src/mint.rs:387`) and the run's clock reading as a `Timestamp`
  (`mint.rs:392`), so two creates would collide; each is refused at start as today.
- A `Decimal`, `Duration`, enum, struct, `Boolean` or `List` identity: `minted` does not mint the
  first four, and a `Boolean` or `List` cannot hold distinct identities; each is refused at start as
  today.
- The spelling of ESS's synthesis witness (`generated-` plus a tick): the host mints a fresh value
  per create, so its identity equals the interpreter's in type, not in text.

## Shared files

- `crates/spindle-kernel/tests/spindle-kernel/main.rs`: one `mod` line, shared with
  `story:guard-over-absent-optional-input`, merged by the coordinator.
- This story keeps out of `crates/spindle-kernel/src/execute.rs`: `identity` still returns a
  `Minted`.
- `CHANGELOG.md` `[Unreleased]`: one bullet per story of wave 25, merged by the coordinator.

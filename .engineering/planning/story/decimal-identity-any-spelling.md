---
format: aep.planning-md/3
id: story:decimal-identity-any-spelling
kind: story
status: active
title: A command names a Decimal identity whatever its spelling
relations:
- serves: vision:spindle
- informed_by: story:integer-identity-any-spelling
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-host/src/world.rs
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/adversary_world_integers.rs
- confidence: cited
  path: crates/spindle-kernel
- confidence: inferred
  path: crates/spindle-kernel/src/execute.rs
- confidence: inferred
  path: crates/spindle-kernel/src/lib.rs
- confidence: inferred
  path: crates/spindle-kernel/src/store.rs
- confidence: cited
  path: crates/spindle-kernel/src/typed.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/adversary_w27_u5.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/decimal_identity.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/main.rs
revision: 12
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T09:52:00Z", actor: "human:operator", revision: 11, decided_on: {"recorded":{"review_outcome":8}}}
- {from: "proposed", to: "active", at: "2026-11-27T09:52:00Z", actor: "human:operator", revision: 12, decided_on: {"recorded":{"review_outcome":8}}}
---
## Outcome

A command names a `Decimal` or `Binary64` identity whatever its spelling, as ESS 0.99.1's interpreter does: a row
created with `1.0` is found by a command with `1`, and the other way round. Today the kernel keys a
row by the identity's JSON text and rewrites only `Integer` values
(`crates/spindle-kernel/src/typed.rs:52`), so the two spellings are two rows in Spindle and one in
ESS. A `Decimal` field keeps its spelling in rows and events; the identity itself is written in one
spelling, because the stores rebuild it from the key (`store.rs:683`).

Found by the wave 27 U5 adversary; red on the base c61a59650, so pre-existing.

## Acceptance

- `a_decimal_identity_names_one_row_whatever_its_spelling` (`adversary_w27_u5`): its `ignore` is
  removed and it passes; the kernel answers as the interpreter at every step.
- `a_decimal_beside_a_decimal_identity_keeps_its_spelling`: a `Decimal` field beside the identity
  keeps `2.0` as written in the row and the event payload; the identity reads `1`.
- `a_durable_store_finds_a_decimal_identity_whatever_its_spelling`: the same on SQLite, reopened
  between the two commands.
- `a_world_row_with_a_decimal_identity_is_found_whatever_its_spelling`: a world row seeded as `1.0`
  is found by a command naming `1`, and the reverse.
- `a_binary64_identity_names_one_row_whatever_its_spelling`: a `Binary64` identity created as
  `1.0` is found by a command naming `1`, and the reverse; the kernel answers as the interpreter.
- `an_established_decimal_identity_is_found_whatever_its_spelling`: a row established through
  `Host::establish` with identity `1.0` is found by a command naming `1.0` and by one naming `1`.

## Not covered

- `Integer` identities: `story:integer-identity-any-spelling`.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel`, the one entry every command takes (`reaching`, `crates/spindle-kernel/src/execute.rs:271`, behind `execute_refusing` `:226` and `execute_reaching` `:247`) — cited
- **Files:** `crates/spindle-kernel/src/typed.rs:36-52` (`integers`/`integral`: only `Integer` is rewritten), named by the story as the defect site; home for, or neighbour of, the function that writes a number identity in one spelling — cited
- **Files:** `crates/spindle-kernel/src/store.rs:678-685` (`key` = `identity.to_string()`, `identity_from_key`): the keying the story describes; under D2=A, home of a `Store` wrapper that writes every identity it is given (`load`, `commit`, `remove`, `commit_all`; the trait is `store.rs:311-481`) in the one spelling and forwards every other method — inferred
- **Files:** `crates/spindle-kernel/src/execute.rs:271-345` (`reaching`): wraps `store` once before `check_input`/`values::related`/`Run`, so the reads at `:353`, `:996`, `:1003`, `:1335`, `:2104`, `values.rs:99`, `:112` and the writes at `execute/set.rs:103-105`, `execute/delete.rs:51-54` all pass it — inferred
- **Also likely:** `crates/spindle-kernel/src/lib.rs` (beside `integers` `:121`), a public re-export of the spelling function for the host — inferred
- **Also likely:** `crates/spindle-host/src/world.rs:115` (a world row's identity already goes through `integers`; it gets the same spelling as a command's key, so `reset_to`'s duplicate check `host.rs:1506` and mint avoidance see it too) — inferred
- **Also likely:** `crates/spindle-host/src/host.rs:1271-1281` (`Host::establish`, which the conformance target calls from `crates/spindle-conformance/src/establish.rs:50`): writes `identity` to the store directly, outside any command — inferred
- **Not touched under D2=A:** `crates/spindle-kernel/src/sqlite.rs` (8 `identity.to_string()` sites incl. the stream id `:416`), `postgres.rs`, `crates/spindle-store-browser/src/indexeddb.rs:815`, `crates/spindle-kernel/src/values.rs`: an identity reaches them already in the one spelling — inferred
- **Symbols:** `typed::integers`, `store::key`, `identity_from_key`, `Store`, `reaching`, `Host::establish`, `world::seed` — cited
- **Tests:** un-ignore `a_decimal_identity_names_one_row_whatever_its_spelling` (`crates/spindle-kernel/tests/spindle-kernel/adversary_w27_u5.rs:590-591`) — cited
- **Tests:** new module `crates/spindle-kernel/tests/spindle-kernel/decimal_identity.rs` plus its `mod` line in `tests/spindle-kernel/main.rs`: `a_decimal_beside_a_decimal_identity_keeps_its_spelling` (identity `1.0`, a `Decimal` field `2.0`: the row field and the event payload answer `2.0`, the scanned identity answers `1`), `a_binary64_identity_names_one_row_whatever_its_spelling` (pins the type-blind key, agreed with the interpreter), `a_durable_store_finds_a_decimal_identity_whatever_its_spelling` (`#[cfg(feature = "sqlite")]`, reopened between the two commands, both directions) — inferred
- **Tests:** `a_world_row_with_a_decimal_identity_is_found_whatever_its_spelling` and `an_established_decimal_identity_is_found_whatever_its_spelling` (through `Host::establish`; `spindle-kernel` has no `spindle-host` dev-dependency) in the existing module `crates/spindle-host/tests/spindle-host/adversary_world_integers.rs`, so no `main.rs` line (seeded `1.0` named by `1`, and the reverse) — inferred
- **Test budget:** each test compiles one small inline model on a memory or scratch SQLite store, like the module's existing tests; expected well under 1 s each, not timed. 60 s per-test bound, 120 s test step, 10 min pool limit — inferred
- **Documents:** `CHANGELOG.md` `[Unreleased]` (and the D3 note on durable rows) — inferred
- **Confidence:** medium. The defect site and the keying are read from the tree. The choice of layer (D2) is a design call, and the completeness of the command path rests on every store access going through the `store` that `reaching` receives.
- **Would collide with:** any unit changing `reaching` or the start of the command path in `crates/spindle-kernel/src/execute.rs` (a different hunk from `before_row`/`when_related`/`related`); any unit changing the `Store` trait or `store.rs` keying; any unit adding a kernel test module (`tests/spindle-kernel/main.rs`); `crates/spindle-host/src/host.rs` near `establish` (wave 29 changes `host.rs` for escalating bindings: sequenced, not concurrent); `crates/spindle-host/src/world.rs` `seed`; `CHANGELOG.md` `[Unreleased]`. Under D2=B add `crates/spindle-kernel/src/values.rs` (wave 29 and story:related-via-nested-input). Under D2=C add `crates/spindle-kernel/src/sqlite.rs` and `crates/spindle-store-browser/src/indexeddb.rs`.
- **Parity fact:** ESS 0.99.1 holds instances in `BTreeMap<QualifiedName, BTreeMap<Node, Instance>>` (`ess-conformance/src/interpret/execute.rs:89`). `Number` orders by exact value whatever its declared type (`ess-primitives/src/facts.rs:955-986`), so `1`, `1.0`, `1e0` and `-0.0`/`0` are one instance for `Decimal` and `Binary64` alike — cited
- **Safety fact:** every row read and write a command makes goes through the `&mut dyn Store` that `reaching` receives, including the host's routing and position wrappers (`host.rs:425`, `:599`, `host/rows.rs:157`), so one wrapper there reaches every backend. Only `Host::establish` and world seeds write outside it. Shown at file:line (level 2), unproven.
- **Depends on:** none. Informed by story:integer-identity-any-spelling (implemented). Wave 29 lands first (`host.rs`, `values.rs`) — cited
- **Architecture:** no new crate, process, wire format or external dependency. Rows already held under a non-canonical spelling of a `Decimal`/`Binary64` identity (`1.0`, `1e0`) are keyed in the new spelling from then on (D3) — inferred

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: an integral number within `i64`/`u64` is written as that integer, any other number
  as the binary64 `serde_json` reads.
- D2 A: a forwarding `Store` wrapper at `reaching`, plus `Host::establish` and the world
  seed.
- D3 A: no migration of rows already held; the CHANGELOG says so.

---
format: aep.planning-md/3
id: story:world-row-integer-fields
kind: story
status: active
title: A row a reset's world loads keeps its Integer fields as integers
relations:
- serves: vision:spindle
- informed_by: story:node-export-serves-a-specification
scope:
- confidence: cited
  path: crates/spindle-host/src/world.rs
- confidence: cited
  path: crates/spindle-node/tests/fixtures/world-exchanges.json
- confidence: inferred
  path: crates/spindle-node/tests/spindle-node/listener.rs
revision: 8
transitions:
- {from: "draft", to: "proposed", at: "2026-11-26T04:51:42Z", actor: "human:operator", revision: 7, decided_on: {"recorded":{"review_outcome":19}}}
- {from: "proposed", to: "active", at: "2026-11-26T04:51:43Z", actor: "human:operator", revision: 8, decided_on: {"recorded":{"review_outcome":19}}}
---
# A row a reset's world loads keeps its Integer fields as integers

## Outcome

A row loaded by a reset's world answers its `Integer` fields as integers, as a row a command
created does. Today they come back as floats: the review fixture's `Submission` created by
`Submit` with `"amount": 12` lists `"amount":12`, and one loaded by a world with `"amount": 1`
lists `"amount":1.0`. Found 2026-11-25 while recording
`crates/spindle-node/tests/fixtures/world-exchanges.json` on the native loopback listener with
`run`'s world reader (`spindle_host::read_world`), so `run`, and every host that loads a world,
answers this way; the recording holds the floats as they are answered today.

The likely place is the conversion of a row's fields from ESS's `Node` to JSON in
`crates/spindle-host/src/world.rs` (`json`), or the store's typing of a seeded row
(`Host::reset_to`): inferred, not traced.

## Acceptance

- `a_world_row_keeps_its_integer_fields` — a reset whose world holds a `Submission` with
  `"amount": 1`, then the view: `"amount":1`, as for a row `Submit` created.
- `crates/spindle-node/tests/fixtures/world-exchanges.json` is recorded again with the integers,
  and the Node lane replays it.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-host`, the reset's world reader — cited
- **Files:** `crates/spindle-host/src/world.rs:100-118` (`seed`, `json`); `json` is a type-blind `serde_json::to_value(node)` — cited
- **Files:** `crates/spindle-node/tests/fixtures/world-exchanges.json` (lines 36 and 54 hold `"amount":1.0` and similar), re-recorded — cited
- **Symbols:** `spindle_host::read_world`, `seed`, `json` in `world.rs` — cited
- **Symbols:** `EssIr::literal_primitive`, `Primitive::Integer`, as the kernel already asks it (`crates/spindle-kernel/src/values.rs:599-600`, `:736-742`) — inferred
- **Also likely:** `crates/spindle-node/tests/spindle-node/listener.rs`, home for `a_world_row_keeps_its_integer_fields` (it compiles the review fixture and calls `read_world`, `:177`) — inferred
- **Not touched:** `crates/spindle-host/src/host.rs` (`Host::reset_to`, `:1194`), which commits the seeded row as given — inferred
- **Documents:** `CHANGELOG.md` — inferred
- **Confidence:** high — ESS 0.99.1's `Number` writer writes an integral value as `f64` (`ess-primitives/src/facts.rs:392-413`)
- **Would collide with:** any unit changing `crates/spindle-host/src/world.rs`, the Node world recording `crates/spindle-node/tests/fixtures/world-exchanges.json`, or `crates/spindle-node/tests/spindle-node/listener.rs`
- **Safety:** every host that loads a world reads it through `spindle_host::read_world` only (`spindle-cli/src/world.rs:23`, `spindle-embed/src/lib.rs:213`), and the only checked-in answer holding a world row's integral float is `world-exchanges.json` — inferred

## Shared files

- `CHANGELOG.md` `[Unreleased]`: one bullet per story of wave 25, merged by the coordinator.

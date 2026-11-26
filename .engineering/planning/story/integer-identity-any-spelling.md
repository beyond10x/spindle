---
format: aep.planning-md/3
id: story:integer-identity-any-spelling
kind: story
status: draft
title: A command names an Integer identity whatever its spelling
relations:
- serves: vision:spindle
- informed_by: story:world-row-integer-fields
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-host/src/world.rs
- confidence: cited
  path: crates/spindle-kernel
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: inferred
  path: crates/spindle-kernel/src/lib.rs
- confidence: inferred
  path: crates/spindle-kernel/src/typed.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/integer_identity.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/main.rs
revision: 5
---
# A command names an Integer identity whatever its spelling

## Outcome

A command whose input names an `Integer` identity as `1.0` finds the row stored as `1`, and the
reverse, as ESS reads both as one value. Today the store keys a row by the identity's JSON text
(`crates/spindle-kernel/src/store.rs:565`) and command input is held as written, so a row created
by a command with `1.0` is not found by a command naming `1`.

Found 2026-11-26 by the adversary of wave 25 U6 (review-result:wave25-adversary-U6-r1, finding 3,
pre-existing). Wave 25 fixes the world half: a reset's world writes every `Integer` as an integer.
This story is the command half, in the kernel.

## Acceptance

- `a_command_names_an_integer_identity_whatever_its_spelling`: a row created with `"shelf_id": 1.0`
  is found by a command with `"shelf_id": 1`, and a row created with `1` by one with `1.0`.
- A created row answers its `Integer` identity and fields as integers whatever the input spelling.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel`, the command entry `execute_refusing` — cited
- **Files:** `crates/spindle-kernel/src/execute.rs:226-305` (`execute_refusing`; input checked at `:252` by `check_input` `:2569`, then used as written for the address `:253-265`, `Run.input` and `values::related` `:300`) — cited
- **Files:** `crates/spindle-kernel/src/typed.rs`, home for the function that writes every `Integer`-typed value of an input as a JSON integer, beside `validate` — inferred
- **Symbols:** `execute_refusing`, `check_input`, `Invocation::input`, `typed::validate`, `EssIr::literal_primitive` — cited
- **Also likely:** `crates/spindle-host/src/world.rs:146-230` (`integers`, wave 25's world half), moved into the kernel and called from there under D1=A — inferred
- **Also likely:** `crates/spindle-kernel/src/lib.rs:104` (beside `admits`), a public re-export of the moved function for `world.rs` under D1=A — inferred
- **Also likely:** `crates/spindle-kernel/tests/spindle-kernel/integer_identity.rs` (new module, inline model with an `Integer` identity, an `Integer`, `Optional<Integer>` and `List<Integer>` field) and its `mod` line in `crates/spindle-kernel/tests/spindle-kernel/main.rs` — inferred
- **Not touched:** `crates/spindle-kernel/src/store.rs:565` (`key`), `sqlite.rs`, `postgres.rs`, `crates/spindle-store-browser/src/indexeddb.rs:773`: each keys by `identity.to_string()`, and a canonical input reaches them already written as an integer — inferred
- **Documents:** `CHANGELOG.md` `[Unreleased]` — inferred
- **Tests:** `a_command_names_an_integer_identity_whatever_its_spelling` (both directions, `1.0` then `1`, `1` then `1.0`, memory store); `a_created_row_answers_its_integer_identity_and_fields_as_integers` (created with `1.0`/`2.0`/`[3.0]`/`4.0`: row, identity and event payload answer integers); under D3=A `an_integer_identity_past_64_bits_is_held_as_written`. Each a small inline model, expected under 1 s, not timed; well inside the 60 s per-test bound and the 120 s step — inferred
- **Confidence:** high — the story names the defect site, and every execute entry funnels into `execute_refusing` (`execute.rs:210`, host `host.rs:1789`)
- **Would collide with:** any unit changing `execute_refusing` or the run's input in `crates/spindle-kernel/src/execute.rs` (story:cross-component-related-reads edits `Run::select`/`Run::row_set` there, and also `:300` if it changes how `values::related` loads rows); any unit adding a kernel test module (`tests/spindle-kernel/main.rs`); under D1=A any unit changing `crates/spindle-host/src/world.rs`; `CHANGELOG.md` `[Unreleased]`
- **Parity fact:** ESS 0.99.1 keys instances by `Node` (`ess-conformance/src/interpret/execute.rs:89`), and `Number` equality compares the exact value (`ess-primitives/src/facts.rs:941-955`), so `1` and `1.0` are one instance — cited
- **Safety fact:** every row write and load a command makes takes its identity from the input after `check_input`, so writing `Integer`-typed values as integers there reaches every store backend and every `Related` key (`values.rs:95`, `:493`). Pointed at `file:line` (step 2), unproven
- **Depends on:** none; story:world-row-integer-fields (implemented) supplied the world half and the `integers` walk — cited

## Decided when scoped

Taken 2026-11-26 on the scoper's recommended option, each reversible, unless noted.

- D1 A: the kernel writes Integer input as integers after `check_input`; the
- D2 A: no migration of durable rows keyed `1.0`; the changelog says so.
- D3 A: an integral Integer past 64 bits stays as written, as ESS admits it.

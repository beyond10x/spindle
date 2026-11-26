---
format: aep.planning-md/3
id: story:integer-identity-any-spelling
kind: story
status: draft
title: A command names an Integer identity whatever its spelling
relations:
- serves: vision:spindle
- informed_by: story:world-row-integer-fields
revision: 1
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

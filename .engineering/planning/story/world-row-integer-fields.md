---
format: aep.planning-md/3
id: story:world-row-integer-fields
kind: story
status: draft
title: A row a reset's world loads keeps its Integer fields as integers
relations:
- serves: vision:spindle
- informed_by: story:node-export-serves-a-specification
revision: 1
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

- `crates/spindle-host/src/world.rs`, `crates/spindle-host/src/host.rs` (inferred).

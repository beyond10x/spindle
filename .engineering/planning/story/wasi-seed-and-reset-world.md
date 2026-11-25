---
format: aep.planning-md/3
id: story:wasi-seed-and-reset-world
kind: story
status: draft
title: A WASI module takes a seed and a reset that carries a world
relations:
- serves: vision:spindle
- decomposes: epic:wasi-server-module
- depends_on: story:wasi-state-survives-instances
revision: 1
---
# A WASI module takes a seed and a reset that carries a world

## Outcome

A WASI module submits the environment's seed once at start, as `run --seed` does, and answers
`POST /api/reset` with a world (rows, decision table, mint) as the native reset route does, with
`run`'s world reader (`spindle_host::read_world` once
`story:node-export-serves-a-specification` moves it there).

## Acceptance

- `a_seed_is_submitted_once` — the seed's rows are listed after start and not doubled by a second
  instance.
- `a_reset_carries_a_world_under_wasmtime` — the exchanges of
  `crates/spindle-node/tests/fixtures/world-exchanges.json` get their recorded answers.

## Depends on

- `story:wasi-state-survives-instances`.

## Scope

- The component crate (inferred).

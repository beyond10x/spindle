---
format: aep.planning-md/3
id: story:execute-commands
kind: story
status: implemented
title: Execute commands against the compiled model
relations:
- decomposes: epic:m0-run-in-process
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-kernel
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T07:28:12Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-04T07:28:12Z", actor: "human:timo", revision: 5}
- {from: "active", to: "implemented", at: "2026-11-04T10:52:45Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":1}}}
---
# Execute commands against the compiled model

## Outcome

A new crate `spindle-kernel` executes a command against an entity instance: it selects the outcome
by the IR's conditions and guards (evaluated through `ess-primitives`, never re-implemented),
applies `creates`/`moves`/`sets`, emits the declared events and returns a declared error on
refusal. State lives behind a store port with a memory and an `eventlog` SQLite implementation,
chosen by the `store.durable` binding. `b10x-spindle run` reads commands as JSON lines.

## Acceptance

- `offer_then_admit_moves_a_deployment_to_admitted` — on Spindle's own `ess/`.
- `admitting_twice_is_the_wrong_state_error`.
- `stop_on_a_stopped_deployment_changes_nothing` — the `refuses: false` outcome.
- `state_survives_a_restart_on_sqlite`.

## Scope

`crates/spindle-kernel/` (new), `crates/spindle-cli/src/main.rs`.

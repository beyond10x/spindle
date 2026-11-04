---
format: aep.planning-md/3
id: epic:m0-run-in-process
kind: epic
status: draft
title: 'M0: run a specification in one process'
relations:
- serves: vision:spindle
revision: 1
---
# M0 — run a specification in one process

## Outcome

`b10x-spindle run <spec> --environment <env>` loads a specification, admits it, and executes
commands against it on one thread, with state in memory or SQLite through `eventlog`.

## Stories

- `story:load-and-compile` — the loader.
- `story:derive-capabilities` — requirements with reasons, per system and per component.
- `story:admit-against-environment` — the environment document and admission.
- `story:execute-commands` — the executor: outcome selection by guard, effects, events.
- `story:spindle-conforms-to-its-specification` — Spindle's own `ess/` held by its conformance
  suite.

## Not in this epic

Channels between processes, timers and microVMs (`epic:m1-m3-rally-in-three-vms`).

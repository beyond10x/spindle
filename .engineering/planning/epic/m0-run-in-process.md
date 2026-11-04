---
format: aep.planning-md/3
id: epic:m0-run-in-process
kind: epic
status: draft
title: 'M0: run a specification in one process'
relations:
- serves: vision:spindle
revision: 2
---
# M0 — run a specification in one process

## Outcome

`b10x-spindle run <spec> --environment <env>` loads a specification, admits it, and executes
commands against it on one thread, with state in memory or SQLite through `eventlog`.

## Stories

Implemented on `main` (`b9b0cd9`):

- `story:load-and-compile` — the loader.
- `story:derive-capabilities` — requirements with reasons, per system and per component.
- `story:admit-against-environment` — the environment document and admission.
- `story:execute-commands` — the executor: outcome selection by guard, effects, events.
- `story:mint-creation-identities` — the runtime mints a creation's identity.
- `story:external-decisions-file` — `--decisions` and `--answer-default`.
- `story:views-with-filters-and-lists` — list views with a state filter and an order.

Open:

- `story:replay-ess-scenarios` — `run --scenario` replays an `ess-scenario/1` timeline (active).
- `story:run-over-http-ingress` — commands and views over HTTP on loopback (active).
- `story:spindle-conforms-to-its-specification` — Spindle's own `ess/` held by its conformance
  suite (draft).

Related, not decomposing this epic: `story:spindle-admission-rules-are-declared` (draft) checks
that `spindle.deployment` declares the admission rules this epic's code implements.

The list was brought up to date by a plan review on 2026-11-04; the first version named only the
first four and the conformance story.

## Not in this epic

Channels between processes, timers and microVMs (`epic:m1-m3-rally-in-three-vms`).

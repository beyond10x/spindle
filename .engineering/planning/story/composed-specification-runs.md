---
format: aep.planning-md/3
id: story:composed-specification-runs
kind: story
status: draft
title: A run serves a composition that ESS merges into one model
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

`run <dir>` serves a composition document that ESS's compiler merges into one model: two systems
with different domain prefixes run in one process, and a binding from an event of one system runs a
command of the other through an actor declared `system: true`.

## Why

A downstream simulation asked ESS to run two specifications in one process with a binding across
them (ESS draft story
`a-binding-maps-an-event-of-one-system-to-a-command-of-another-system-in-one-run`). Spindle loads
one compiled model; the merge happens in ESS's compiler.

## Architecture decision

Option A, picked by the operator on 2026-11-27 17:10 CET: ESS's compiler merges the composition
document into one model, and Spindle reads it as it reads `system.yaml` today. The archived
`story:one-ingress-several-specifications` stays archived.

Not taken: B (Spindle loads two models and joins them itself), C (no change; two processes with
events over the webhook egress).

## Acceptance

- The ESS pin in `Cargo.toml` names the ESS release that merges a composition document.
- `a_composed_run_carries_an_event_of_one_system_to_a_command_of_the_other`: two neutral systems
  with different prefixes, one composition document; a command of the first publishes an event,
  and the bound command of the second runs and its row is readable over the second system's route.

## Not covered

- Waits on the ESS release; it enters no wave before that release exists.

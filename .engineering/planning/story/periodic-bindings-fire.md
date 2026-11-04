---
format: aep.planning-md/3
id: story:periodic-bindings-fire
kind: story
status: draft
title: Periodic bindings fire
relations:
- decomposes: epic:m1-m3-rally-in-three-vms
revision: 1
---
# Periodic bindings fire

## Outcome

A host loop drives every periodic binding through the bound `timer` and `clock` implementations,
honouring the binding's declared profile (`every`, `first`, `overlap`, `missed`).

## Acceptance

- `a_periodic_binding_invokes_its_command_every_period` — with a test clock.
- `overlap_serial_per_instance_never_runs_two_ticks_at_once`.
- `missed_ticks_keep_one_pending_and_drop_the_rest`.

## Scope

`crates/spindle-host/` (new).

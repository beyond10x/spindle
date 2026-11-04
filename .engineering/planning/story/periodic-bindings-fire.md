---
format: aep.planning-md/3
id: story:periodic-bindings-fire
kind: story
status: active
title: Periodic bindings fire
relations:
- decomposes: epic:m1-m3-rally-in-three-vms
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-timer
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T07:28:12Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-04T07:28:12Z", actor: "human:timo", revision: 5}
---
# Periodic bindings fire

## Outcome

A new crate `spindle-timer` schedules periodic bindings from the compiled IR
(`ResolvedBinding` with a periodic cause) against a `Clock` port, honouring the binding's declared
profile (`every`, `first`, `overlap`, `missed`), and yields due ticks as `(binding, command)`
pairs. It knows nothing about the executor: the host (`story:rally-is-watchable-in-a-terminal`)
turns a due tick into a command.

## Acceptance

- `a_periodic_binding_is_due_every_period` — with a test clock.
- `first_after_period_is_not_due_at_activation`.
- `overlap_serial_per_instance_never_yields_a_second_tick_while_one_is_open`.
- `missed_ticks_keep_one_pending_and_drop_the_rest`.

## Scope

`crates/spindle-timer/` (new). Nothing else.

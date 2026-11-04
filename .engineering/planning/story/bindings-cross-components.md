---
format: aep.planning-md/3
id: story:bindings-cross-components
kind: story
status: draft
title: Bindings cross components
relations:
- decomposes: epic:m1-m3-rally-in-three-vms
revision: 1
---
# Bindings cross components

## Outcome

An event published by one component reaches the command it binds to in another component through
the bound `channel` implementation: `in-process` or `tcp`. Delivery follows the binding's declared
`delivery` and `on_failure`; events leave through an outbox after the state that produced them is
durable.

## Acceptance

- `an_event_reaches_the_bound_command_in_process`.
- `an_event_reaches_the_bound_command_over_tcp`.
- `at_least_once_redelivers_after_a_lost_acknowledgement`.
- `nothing_leaves_the_outbox_before_the_state_is_durable`.

## Scope

`crates/spindle-host/`.

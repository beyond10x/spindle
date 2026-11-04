---
format: aep.planning-md/3
id: story:bindings-cross-components
kind: story
status: active
title: Bindings cross components
relations:
- decomposes: epic:m1-m3-rally-in-three-vms
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-channel
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T07:28:12Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-04T07:28:12Z", actor: "human:timo", revision: 5}
---
# Bindings cross components

## Outcome

A new crate `spindle-channel` carries an event published by one component to the command it binds
to in another. It routes from the compiled IR (`ResolvedBinding` with an event cause, its
`mapping`, `delivery` and `on_failure`) and offers two implementations behind one `Channel` port:
`in-process` and `tcp` (length-prefixed JSON frames). An outbox holds events until the producer
marks their state durable; delivery follows the declared `at_most_once` / `at_least_once`.

## Acceptance

- `an_event_is_mapped_to_the_bound_command_input` — mapping `event.<field>` to command fields.
- `an_event_reaches_the_bound_component_in_process`.
- `an_event_reaches_the_bound_component_over_tcp` — loopback.
- `at_least_once_redelivers_after_a_lost_acknowledgement`.
- `nothing_leaves_the_outbox_before_the_state_is_durable`.

## Scope

`crates/spindle-channel/` (new). Nothing else.

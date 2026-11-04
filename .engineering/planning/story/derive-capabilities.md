---
format: aep.planning-md/3
id: story:derive-capabilities
kind: story
status: draft
title: Derive capabilities from the compiled model
relations:
- decomposes: epic:m0-run-in-process
revision: 1
---
# Derive capabilities from the compiled model

## Outcome

`spindle_capability::derive(ir, component)` returns every capability the system, or one
component of it, requires, each with the declarations that require it. Nobody writes a capability
down.

## Acceptance

- `entities_need_a_durable_store_and_an_external_outcome_needs_a_decider`.
- `a_periodic_cause_needs_a_timer_and_a_clock_and_a_cross_component_binding_a_channel`.
- `each_component_needs_only_what_it_runs`.
- `an_unknown_component_is_none`.

## Scope

`crates/spindle-capability/src/derive.rs`, `crates/spindle-capability/tests/`.

---
format: aep.planning-md/3
id: story:derive-capabilities
kind: story
status: implemented
title: Derive capabilities from the compiled model
relations:
- decomposes: epic:m0-run-in-process
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T18:28:15Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-11-04T18:28:15Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-11-04T18:28:15Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
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

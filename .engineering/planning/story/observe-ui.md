---
format: aep.planning-md/3
id: story:observe-ui
kind: story
status: implemented
title: The observe UI
relations:
- serves: vision:spindle
- decomposes: epic:observe
- depends_on: story:observe-api-contract
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-04T16:05:55Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
# The observe UI

## Outcome

A Vue 3 UI embedded in `b10x-spindle` shows topology, instance and view screens; rally's `MatchState` is drawn as the board. API types are generated from Rust and drift-checked. Design § 6.

## Acceptance

- `the_topology_shows_one_card_per_component_and_one_edge_per_binding`
- `the_ui_types_match_the_api`

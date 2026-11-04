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
scope:
- confidence: cited
  path: ui
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-04T16:05:55Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
# The observe UI

## Outcome

A Vue 3 UI in `ui/` shows the topology, instance and view screens; rally's `MatchState` is drawn
as the board. API types are generated from Rust and drift-checked. Design § 6.

As shipped, the UI runs on the contract's fixtures only: `pnpm dev` serves the observe API from
`ess/fixtures/observe/` through the Vite plugin `ui/dev/fixtures-api.ts` (token `dev`). The built
bundle is not embedded in `b10x-spindle`, and nothing serves it outside `pnpm dev`.

## Moved out

"Embed the built UI in `b10x-spindle` and serve it at `GET /`" was in this story's outcome and
did not ship. It moved to `story:control-process-serves-the-api` (acceptance
`the_ui_is_served_from_the_binary`), because only the control process has a server to serve it
from. Recorded 2026-11-04 by a plan review against `main` at `b9b0cd9`.

## Acceptance

- `the_topology_shows_one_card_per_component_and_one_edge_per_binding`
  (`ui/src/screens/TopologyScreen.test.ts`).
- `the_ui_types_match_the_api` (`ui/src/api/types.test.ts`, a `describe` block).
- The board: `ui/src/lib/board.test.ts`, `describe('the rally board')`, tests
  `places the ball and both paddles on a 16 by 9 grid` and `draws the fixture MatchState`. They
  test `ui/src/lib/board.ts`, which `ui/src/components/RallyBoard.vue` renders; the component
  itself has no mount test.

## Gap (round-1 acceptance critic, 2026-11-04)

`ui/src/screens/InstanceScreen.vue` and `ui/src/screens/ViewScreen.vue` exist on `main` at
`b9b0cd9` with no test that renders them, so this story's "instance and view screens" is asserted,
not checked. The two named checks are carried to `story:control-process-serves-the-api`
(`the_instance_screen_shows_one_instances_counters_and_quota`,
`the_view_screen_renders_a_declared_view_as_a_table`), which already owns the rest of the UI's
open work. This story's status is the operator's to revisit.

## Scope

- `ui/` — cited (the tests above and `ui/dev/fixtures-api.ts` live there).

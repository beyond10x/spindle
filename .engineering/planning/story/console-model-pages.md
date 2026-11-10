---
format: aep.planning-md/3
id: story:console-model-pages
kind: story
status: draft
title: The observability console generates every model page from the model and its families
relations:
- serves: vision:spindle
- decomposes: epic:observability-console
- depends_on: story:console-pages-specified
- depends_on: story:console-server
- depends_on: story:metric-families-route
- depends_on: story:store-and-kernel-durations
- depends_on: story:view-read-families
- depends_on: story:live-topology
scope:
- confidence: inferred
  path: crates/spindle-console/tests
- confidence: cited
  path: ui/src/components/Bars.vue
- confidence: cited
  path: ui/src/components/Sparkline.vue
- confidence: inferred
  path: ui/src/console/fixtures
- confidence: inferred
  path: ui/src/console/pages
- confidence: cited
  path: ui/src/lib/rates.ts
- confidence: cited
  path: ui/src/lib/topology.ts
revision: 4
---
# The observability console generates every model page from the model and its families

## Outcome

For each configured model, the console reads its topology, its `/api/model` and its
`MetricFamilies`, and derives its pages as `story:console-pages-specified` specifies: system,
domain, component, command, view, event, external cause and store. Each page shows rate, errors and
duration p50, p95 and p99 over a chosen time range, from PromQL the console builds from the family
description and the label vocabulary. Navigation follows the topology's edges and the IR's
components. Charts are SVG with no chart library, as `docs/design/observe.md` § 6 rules.

## Acceptance

- `every_page_renders_for_two_models` (vitest, against a recorded Prometheus fixture): for
  `examples/rally/ess` and a second neutral fixture, every page of the derived page list renders
  with data and no error, with no code or configuration change between the two.
- `queries_are_built_from_the_families`: removing a family from the recorded `MetricFamilies`
  removes its chart and changes no other.
- `a_new_command_gets_its_page`: adding a command to the second fixture adds its page and nothing
  else changes.
- `a_missing_series_shows_no_data`: a family with no samples in the range renders "no data", not an
  error.

## Scope

- `ui/src/console/` (new: pages, PromQL builder, router), reusing `ui/src/components/Sparkline.vue`,
  `Bars.vue`, `ui/src/lib/topology.ts`, `ui/src/lib/rates.ts`.
- `ui/src/console/fixtures/` (recorded Prometheus answers, two models' topology, IR and families).
- `crates/spindle-console/tests/` (the server answers the same fixtures end to end).

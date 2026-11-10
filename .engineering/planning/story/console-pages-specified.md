---
format: aep.planning-md/3
id: story:console-pages-specified
kind: story
status: draft
title: The observability console's sources and pages are specified in spindle.observe
relations:
- serves: vision:spindle
- decomposes: epic:observability-console
- depends_on: story:observe-label-vocabulary
scope:
- confidence: cited
  path: Taskfile.yml
- confidence: cited
  path: crates/spindle-observe-api
- confidence: cited
  path: ess/domains/observe.yaml
- confidence: inferred
  path: ess/fixtures/observe/console
- confidence: cited
  path: ui/src/api/generated
revision: 4
---
# The observability console's sources and pages are specified in spindle.observe

## Outcome

Guardrail 7: a console page and its sources have no home in any ESS document of this repository.
`spindle.observe` declares them before the console is built:

- a console source: one Prometheus URL and one or more model URLs;
- a page kind for system, domain, component, command, view, event, external, store, and the
  request page;
- what a page shows: per series, rate, errors and the p50, p95 and p99 of the family whose `level`
  matches the page, chosen from `MetricFamilies` by `level`, `kind` and `unit`, never by a family
  name written into the console;
- where each page comes from, for one model:
  - domain, command, view, event and external pages: one per node of that kind in the model's
    topology (`spindle.observe.NodeKind`, `ess/domains/observe.yaml:516-524`);
  - component pages: one per component of the IR the model URL answers at `/api/model` (the
    topology has no component node);
  - store pages: one per component that the families report at level `store` (the topology has no
    store node, and a model URL does not expose its environment's store binding);
  - one system page and one request page per model.

## Acceptance

- `ess specify validate --path ess` accepts the console nouns.
- `ess/fixtures/observe/console/rally-pages.json`, the page list derived for rally, validates
  against the generated schema.
- `rally-pages.json` holds one page per command, view, event and external cause of rally's
  topology.
- `rally-pages.json` holds one component page per rally component: `rally-table`, `rally-player-west` and
  `rally-player-east`.
- `rally-pages.json` holds one store page per rally component whose families report level `store`.
- `task check:the_generated_types_have_no_drift` passes with the new Rust and TypeScript types.

## Scope

- `ess/domains/observe.yaml` (beside `MetricFamily`, line 429, and `NodeKind`, line 516).
- `ess/fixtures/observe/console/` (new), `crates/spindle-observe-api/`, `ui/src/api/generated/`.
- `Taskfile.yml` (`OBSERVE_ROOTS`, if the fixtures need a new root).

## Shared files

`story:trace-nouns-specified` also edits `ess/domains/observe.yaml`, `crates/spindle-observe-api/`
and `ui/src/api/generated/`; it runs after this story.

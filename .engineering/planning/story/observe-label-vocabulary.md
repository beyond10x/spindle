---
format: aep.planning-md/3
id: story:observe-label-vocabulary
kind: story
status: draft
title: The metric label vocabulary and the family description are specified in spindle.observe
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability-depth
scope:
- confidence: cited
  path: crates/spindle-ingress/src/observe/metrics.rs
- confidence: cited
  path: crates/spindle-observe-api
- confidence: cited
  path: ess/domains/observe.yaml
- confidence: cited
  path: ess/fixtures/observe/metrics/families.json
- confidence: cited
  path: ui/src/api/generated
revision: 3
---
# The metric label vocabulary and the family description are specified in spindle.observe

## Outcome

`spindle.observe` says, before any new family is built, what a family is labelled with and how a
reader can tell what it measures:

- a label vocabulary, read from the model: `domain`, `component`, `kind` (`entity`, `command`,
  `view`, `event`, `external`, `binding`) and `name`, each value a declared name of the IR;
- `MetricFamily` gains `level` (`ingress`, `admission`, `command`, `store`, `view`, `binding`,
  `external`, `timer`, `queue`, `deployment`) and `unit` (`seconds`, `bytes`, `rows`, `envelopes`,
  `ticks`, `requests`, none);
- a stated rule: no label value is an entity identity, an actor, a path value or anything else the
  model does not declare, so every family's cardinality is bounded by the specification.

The 14 existing families keep their names and labels; the fixture gains `level` and `unit` for
each, and the labels the vocabulary adds to them where they name a model construct.

## Acceptance

- `ess specify validate --path ess` accepts `ess/domains/observe.yaml` with the vocabulary and the
  two new fields.
- `ess/fixtures/observe/metrics/families.json` validates against the generated schema.
- `ess/fixtures/observe/metrics/families.json` lists all 14 existing families.
- Every family in `ess/fixtures/observe/metrics/families.json` has a `level` and a `unit`.
- `task check:the_generated_types_have_no_drift` passes with the regenerated Rust and TypeScript
  types.
- The unit test that holds `FAMILIES` equal to the fixture (`metrics.rs:36-37`) still passes.

## Scope

- `ess/domains/observe.yaml` (`spindle.observe.MetricFamily`, `MetricFamilies`, lines 429-452).
- `ess/fixtures/observe/metrics/families.json`.
- `crates/spindle-observe-api/` and `ui/src/api/generated/` (generated, `task generate:observe`).
- `crates/spindle-ingress/src/observe/metrics.rs` (`Family`, `FAMILIES`: `level` and `unit`).

## Notes

Parallel safety: `story:live-topology` (active) edits `ess/domains/observe.yaml`; do not run beside
it. The model's word for a bounded context is a domain (`spindle.observe.NodeKind`), so the label
is `domain`, not `context`.

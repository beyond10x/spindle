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
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-ingress/src/observe/metrics.rs
- confidence: cited
  path: crates/spindle-observe-api
- confidence: inferred
  path: docs/design/observe.md
- confidence: cited
  path: ess/domains/observe.yaml
- confidence: cited
  path: ess/fixtures/observe/metrics/families.json
- confidence: cited
  path: ess/generated/jsonschema
- confidence: cited
  path: ui/src/api/generated
revision: 10
---
# The metric label vocabulary and the family description are specified in spindle.observe

## Outcome

`spindle.observe` says, before any new family is built, what a family is labelled with and how a
reader can tell what it measures:

- a label vocabulary, read from the model: `domain`, `component`, `kind` (`entity`, `command`,
  `view`, `event`, `external`, `binding`) and `name`, each value a declared name of the IR;
- `MetricFamily` gains two required fields, `level` (`ingress`, `admission`, `command`, `store`,
  `view`, `binding`, `external`, `timer`, `queue`, `deployment`) and `unit` (`seconds`, `bytes`,
  `rows`, `envelopes`, `ticks`, `requests`, `"none"`); a family that counts things the other units
  do not name carries `"unit": "none"` (D5 A);
- a stated rule, in the model's text: no label value is an entity identity, an actor, a path value
  or anything else the model does not declare, so every family's cardinality is bounded by the
  specification.

The 18 existing families keep their names and labels (D1 A), so the exposition does not change; the
fixture and `FAMILIES` gain `level` and `unit` for each (D3 A, D5 A). The vocabulary's labels are
the ones families built from now on carry.

## Why

Line numbers at b546d6d2.

This is the first step of the observability, console and Kubernetes chain
(`epic:runtime-observability-depth`): eight draft stories `depends_on` it
(`store-and-kernel-durations`, `binding-external-timer-durations`, `view-read-families`,
`queue-families`, `trace-nouns-specified`, `metric-families-route`, `console-pages-specified`,
`control-process-metrics`), each building a family or a page from the vocabulary and the two fields.

The story was written on 2026-11-10 against 14 families. Four were added since:
`spindle_bound_refusals_total`, `spindle_seed_commands_total`,
`spindle_metrics_scrape_duration_seconds` and `spindle_metrics_series_mode`
(`ess/fixtures/observe/metrics/families.json`, 18 families; `FAMILIES: [Family; 18]`,
`crates/spindle-ingress/src/observe/metrics.rs:57`). `story:metrics-series-bounded` (2026-11-27)
already lists in the code which labels take their values from the specification
(`SPECIFIED_LABELS`, `metrics.rs:227-235`); the vocabulary states that rule in the model.

## Acceptance

The story adds one new and one changed unit test to the default test step
(`task check:the_tests_pass_within_the_budget`), each under 1 s, and no test to the PostgreSQL lane.

- `metrics::tests::the_label_vocabulary_is_the_specified_vocabulary` (new, in the unit tests of
  `crates/spindle-ingress/src/observe/metrics.rs`, `:645`), default test step: it reads the
  generated schema of the vocabulary,
  `ess/generated/jsonschema/schema/types/spindle.observe.MetricLabel.schema.json`, from
  `CARGO_MANIFEST_DIR` as `specified()` (`:651-659`) reads the fixture, and asserts that its
  `enum` is `["domain", "component", "kind", "name"]`, that the schema of the `kind` label's values
  (`spindle.observe.MetricLabelKind`) has the `enum` `["entity", "command", "view", "event",
  "external", "binding"]`, and that the vocabulary's `description` (its `naming.summary` in
  `observe.yaml`) holds the rule's sentence: no label value is an entity identity, an actor, a path
  value or anything else the model does not declare. It fails on today's tree, where neither schema
  file exists. It reads the JSON file because `spindle-ingress` does not depend on
  `crates/spindle-observe-api` (only `spindle-cli` does, `crates/spindle-cli/Cargo.toml:54`), where the generated Rust
  type is `MetricLabel`; it compares with a literal list, not with `SPECIFIED_LABELS`
  (`metrics.rs:227-235`), which holds the seven labels an aggregated scrape writes empty (`route`,
  `operation`, `command`, `outcome`, `event`, `binding`, `entity`) and none of the vocabulary's.
  The type names are this story's; an implementor who names them otherwise reads those files.
- `metrics::tests::an_aggregated_scrape_drops_the_specified_labels` (`metrics.rs:712-802`), default
  test step: passes unchanged. It walks `FAMILIES` and asserts every label of every family is one of
  `SPECIFIED_LABELS` or the test's `KEPT_LABELS` (`:720-724`), each a label with a bounded set of
  values, so the cardinality rule holds over the 18 existing families (D1 A).
- `task check:the_observe_domain_validates` (`ess specify validate --path ess --strict-requires`)
  accepts `ess/domains/observe.yaml` with the vocabulary, the label rule and the two new required
  fields of `spindle.observe.MetricFamily`.
- `task check:every_fixture_answer_validates_against_its_schema` (`Taskfile.yml:347`):
  `ess/fixtures/observe/metrics/families.json` validates against the regenerated
  `spindle.observe.MetricFamilies` schema, in which `level` and `unit` are required.
- `metrics::tests::the_families_are_the_specified_families`
  (`crates/spindle-ingress/src/observe/metrics.rs:661-675`), default test step: `FAMILIES`, with
  `level` and `unit` in the compared JSON, equals the fixture, which lists all 18 existing families,
  each with a `level` and a `unit` from the specified values; a family that counts things the other
  units do not name has `"unit": "none"`, in the fixture and in `FAMILIES` alike (D5 A).
- `observe::the_exposed_families_are_the_specified_families`
  (`crates/spindle-cli/tests/spindle-cli/observe.rs:993`), default test step: passes unchanged, so
  every family keeps its name, type and labels (D1 A).
- `task check:the_generated_types_have_no_drift` passes with the schema, Rust and TypeScript types
  regenerated by `task generate:observe` (`Taskfile.yml:312-317`), none edited by hand.
- The change goes under `[Unreleased]` `### Added` in `CHANGELOG.md`.

## Decided when scoped

Taken 2026-11-28 on the operator's pick (D-W35 A, relayed by the host operator): option A of every
decision, the one the Notes recommend or the smallest, each reversible. No new dependency.

- D1 The labels of the 18 existing families: A, every family keeps its labels; the vocabulary
  governs the families built from now on, and the fixture gains only `level` and `unit`, so
  nothing in the exposition changes.
  Not taken: B (add the vocabulary's `domain`, `kind` and `name` labels to existing families where
  they name a model construct, which changes the exposition, `metrics.rs` and the label tests of
  `observe.rs`), C (rename existing labels such as `command` to `kind` and `name`, which breaks
  every query a consumer wrote).
- D2 The label for a bounded context: A, `domain`, the model's word (`spindle.observe.NodeKind`,
  `ess/domains/observe.yaml:548-556`).
  Not taken: B (`context`).
- D3 The level of the families the list does not name directly: A, each takes a level from the
  story's list, which stays at ten levels: `spindle_bound_refusals_total` `binding`,
  `spindle_seed_commands_total` `command`, `spindle_resets_total` `store`,
  `spindle_metrics_scrape_duration_seconds` and `spindle_metrics_series_mode` `ingress`; a family
  that counts things the other units do not name has `"unit": "none"` (D5 A).
  Not taken: B (new levels `seed` and `observe` for these families), C (`level` optional and
  absent for them).
- D4 Running beside another story on `ess/domains/observe.yaml`: A, built beside
  `story:unknown-commit-in-seed-and-retries` (wave 35, U3) and merged first; U3 regenerates the
  generated files on top of this story's merged change. U3 `depends_on` this story (edge added at
  the wave's review round 1), so the order is in the graph, not only in prose.
  `story:live-topology`, which the Notes named, is implemented and no longer edits the file.
  Not taken: B (U3 starts only once this story is merged), C (U3 takes its fallbacks D2 C and D6 C
  and leaves `observe.yaml`).
- D5 The unit of a family that counts things (taken at the wave's review round 1): A, `unit` is
  required on every family, and its enumeration holds an explicit `"none"`, which the fixture and
  `FAMILIES` write for a family that counts things the other units do not name.
  Not taken: B (the field is absent for such a family).

## Scope

Checked 2026-11-28 against b546d6d2, file by file; revision 3's section had no scoper report and
read the tree of 2026-11-10. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `ess/domains/observe.yaml` — `spindle.observe.MetricKind` `:517-521`,
  `MetricFamily` `:523-538`, `MetricFamilies` `:540-546` (revision 3 said `:429-452`, stale); the
  vocabulary and the label rule are declared beside them — cited
- **Files:** `ess/fixtures/observe/metrics/families.json` — 18 families (revision 3 said 14), each
  gaining `level` and `unit`; validated at `Taskfile.yml:347` — cited
- **Files:** `crates/spindle-ingress/src/observe/metrics.rs` — `Family` `:46-53` and `FAMILIES`
  `:55-191` gain `level` and `unit`; the unit test `the_families_are_the_specified_families`
  `:661-675` compares `name`, `kind`, `labels` and `summary` to the fixture and gains the two
  fields — cited
- **Generated, not edited by hand:** `ess/generated/jsonschema` (the `MetricFamily` and
  `MetricFamilies` schemas, new type files and `.ess-output/state.json`),
  `crates/spindle-observe-api` (`source.schema.json`, `types.rs`, `types-report.json`,
  `.ess-output/state.json`) and `ui/src/api/generated` (`source.schema.json`, `types.ts`,
  `types-report.json`, `.ess-output/state.json`), all by `task generate:observe`
  (`Taskfile.yml:312-317`) — cited
- **Read, not touched under D1 A:** `crates/spindle-cli/tests/spindle-cli/observe.rs:739-760`
  reads only `name`, `kind` and `labels` of the fixture; `SPECIFIED_LABELS` (`metrics.rs:227-235`)
  and the test's `KEPT_LABELS` (`:677-707`), whose `kind` label of `spindle_http_requests_total`
  holds `command` or `view`, both values of the vocabulary's `kind` — cited
- **Also likely:** `docs/design/observe.md:288-291` ("The metric families are a contract": name,
  kind and order) gains `level` and `unit` — inferred; `README.md:136` lists the families and is
  unchanged under D1 A — cited
- **Also likely:** `CHANGELOG.md` `[Unreleased]`, one line — inferred
- **Confidence:** high for the specification, fixture and `metrics.rs`; the generated files follow
  from `task generate:observe`
- **Would collide with:** `story:unknown-commit-in-seed-and-retries` (wave 35, U3) on
  `ess/domains/observe.yaml` (`DeadLetter` `:876-927`, `SeedState` `:949-987`, other types than
  this story's) and on every generated file above, `.ess-output/state.json` included: U3 merges
  after this story and regenerates on top of it (D4 A); `CHANGELOG.md` with every unit of the wave
- **Test time:** one existing unit test changes and one new unit test
  (`the_label_vocabulary_is_the_specified_vocabulary`) reads one schema file, each well under 1 s; no PostgreSQL-lane
  test — inferred

## Notes

Revision 3's two notes are decisions now: the parallel-safety note is D4, the label name D2.

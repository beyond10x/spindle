---
format: aep.planning-md/3
id: story:metrics-series-bounded
kind: story
status: draft
title: An operator bounds the series a metrics scrape writes
relations:
- decomposes: epic:runtime-observability-depth
- serves: vision:spindle
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: README.md
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/observe.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe/metrics.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe/topology.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
- confidence: inferred
  path: ess/fixtures/observe/metrics/families.json
- confidence: cited
  path: website/docs/concepts/observe.md
- confidence: cited
  path: website/docs/reference/cli.md
revision: 6
---
## Outcome

An operator can bound how many series `GET /__spindle/metrics` writes for a large specification.
The operator chooses whether commands are counted one series each or aggregated, and the scrape
states which was chosen.

## Why

Measured on 0.8.0 by a downstream deployment, reported 2026-11-10: on its bundle one scrape of
`/__spindle/metrics` wrote 12,225 series in 1,198,629 bytes. Commands are labelled by qualified
name: `Series` is method, route template, operation, kind, class and status class
(`crates/spindle-ingress/src/observe.rs:446-448`). External causes are counted by command, outcome
and source (`observe.rs:468`). The family text is written in
`crates/spindle-ingress/src/observe/metrics.rs`.

Every label value comes from the specification, so the count is bounded, but the bound grows with
the specification's size. The downstream keeps `--observe` off in its dev deployment for this reason
and because of `story:refusal-reason-bounded`.

## Acceptance

- `an_aggregated_scrape_drops_the_specified_labels` (unit, `metrics.rs`): in the aggregated mode
  every label whose value comes from the specification (`route`, `operation`, `command`,
  `outcome`, `event`, `binding`, `entity`) is written empty, and equal label sets are summed.
- `an_aggregated_scrape_of_300_commands_writes_at_most_n_series`: a served run over a specification
  with 300 commands, each answered once, with `--observe-series aggregated`, writes at most the
  number of series the test states, and that number does not grow with the number of commands.
  The specification is generated once per test run, not per test.
- `a_scrape_names_its_series_mode`: the scrape holds `spindle_metrics_series_mode{mode=...} 1` with
  the mode in effect, in both modes.
- `observe_series_requires_observe`: `--observe-series` without `--observe` exits 2.
- On the default (`per-command`) the series of a scrape are those of 0.22.0 plus the mode family;
  the existing scrape tests stay green unchanged, and the topology's route edges still count.
- `website/docs/concepts/observe.md` states the series bound of each mode.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-ingress` metrics exposition — cited
- **Files:** `crates/spindle-ingress/src/observe/metrics.rs:390-468` (`exposition`, `Samples::add` at
  245, `FAMILIES` at 50) — cited
- **Files:** `crates/spindle-ingress/src/observe.rs:578-676` (`Ring`, `Recorder::new`; the mode is
  stored beside the counts) — cited
- **Files:** `crates/spindle-ingress/src/surface.rs:1468-1483` (`Surface::observing`; a new builder
  next to it, the `observing(capacity)` signature kept, since 7 test call sites use it) — cited
- **Files:** `crates/spindle-cli/src/cli.rs:327-338` (`observe`, `observe_requests`; the new flag
  sits beside them, `requires = "observe"`) — cited
- **Files:** `crates/spindle-cli/src/main.rs:213-229,470` and `crates/spindle-cli/src/serve.rs:109,
  368-369` (pass the mode through `Execute` to `surface.observing`) — cited
- **Files:** `ess/fixtures/observe/metrics/families.json` (gains the mode family under D2-A) —
  inferred
- **Symbols:** `exposition`, `Samples::add`, `FAMILIES`, `Ring`, `Recorder::new`,
  `Surface::observing`, `Execute::observe` — cited
- **Tests:** `crates/spindle-ingress/src/observe/metrics.rs` unit tests:
  `an_aggregated_scrape_drops_the_specified_labels` (pure label mapping, well under 1 s) — inferred
- **Tests:** `crates/spindle-cli/tests/spindle-cli/observe.rs` (helpers `serve`, `scrape`,
  `families`): `an_aggregated_scrape_of_300_commands_writes_at_most_n_series` (states N; spec
  generated in the test into a temp dir; compile and serve time unmeasured, must be timed before it
  is added, 60 s bound), `a_scrape_names_its_series_mode`, `observe_series_requires_observe` — inferred
- **Tests:** existing guards stay green unchanged: `observe.rs:843-1043` and
  `adversary_prometheus.rs:185-235` (default series), `metrics.rs:543`
  `the_families_are_the_specified_families`, `topology.rs:473-486` (route edges) — cited
- **Also likely:** `crates/spindle-ingress/src/observe/topology.rs:322-333` read-only; it must keep
  receiving full labels — cited
- **Documents:** `website/docs/concepts/observe.md:47-64` (the series bound of each mode),
  `website/docs/reference/cli.md` (regenerated by `spindle-docs`), `README.md:109`, `CHANGELOG.md` — cited
- **Confidence:** medium — the files are read from the tree, but which families the aggregated mode
  collapses (D1) and how the scrape names the mode (D2) are open, and they decide whether
  `families.json` and `ess/domains/observe.yaml` are touched
- **Would collide with:** the `run` flag block of `crates/spindle-cli/src/cli.rs` and the regenerated
  `website/docs/reference/cli.md`; the `Execute` struct and run arm of `crates/spindle-cli/src/main.rs`;
  the surface setup in `crates/spindle-cli/src/serve.rs:360-370`; the `Surface` builder block of
  `crates/spindle-ingress/src/surface.rs`; `Ring` / `Recorder` in `crates/spindle-ingress/src/observe.rs`;
  the `FAMILIES` table and `ess/fixtures/observe/metrics/families.json`; `website/docs/concepts/observe.md`
- **Safety fact:** the aggregation happens when the scrape is written (`metrics.rs` `exposition`, where
  `Samples::add` already sums equal label sets at 253-254), never in `Recorder::record`
  (`observe.rs:739-748`), because `topology.rs:324-333` matches `Traffic.requests` on route and
  operation to count each `routes` edge. Aggregating at record time would zero every route edge — step 2, unproven
- **Safety fact:** the ESS 0.99.1 interpreter has no metrics; `ess-conformance` at `d7833ee` has no
  `metric` or `prometheus` term, so no parity target applies — step 2, unproven

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 C: the aggregated mode writes every label whose value comes from the
  specification (`route`, `operation`, `command`, `outcome`, `event`, `binding`, `entity`) as the
  empty value at scrape time in `exposition`, so the series count no longer depends on the
  specification; `Recorder::record` keeps full labels, as the topology's route edges read them.
- D2 A: a new family `spindle_metrics_series_mode{mode}`, a gauge always 1, added to
  `FAMILIES` and `ess/fixtures/observe/metrics/families.json`.
- D3 A: `run --observe-series <per-command|aggregated>`, default `per-command`,
  `requires = "observe"`.

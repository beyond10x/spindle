---
format: aep.planning-md/3
id: story:view-aggregate-fields
kind: story
status: active
title: A view with aggregate fields is served
tags:
- downstream-gap
relations:
- serves: vision:spindle
- decomposes: epic:downstream-kernel-constructs
scope:
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-host/src/view.rs
- confidence: cited
  path: website/docs/concepts/specification.md
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T22:14:36Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T22:14:36Z", actor: "human:timo", revision: 3}
---
# A view with aggregate fields is served

## Outcome

A view that declares `group_by:` and fields with `aggregate:` (`count`, `sum`, with `where:`)
answers its rows with the computed values. Today aggregation stays refused as unsupported
(`story:views-with-filters-and-lists`: "Parameters, computed fields, aggregation and paging stay
refused as Unsupported"). A downstream specification has 44 such views (its count).

## Acceptance

On a neutral fixture with a view over `Session` filtered `state == Open`, grouped by `account_id`,
with `open_sessions: {aggregate: {count: {}}}`, the view answers one row per account with its count
of open sessions, equal to ESS 0.90.0's interpreter.

## Scope

Derived 2026-11-10 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-host` (view reader) — cited; `crates/spindle-host/src/view.rs:51` refuses `view.aggregation.is_some()` as "an aggregation"
- **Files:** `crates/spindle-host/src/view.rs:48` (`unsupported`, the refusal arm at 51-52) — cited
- **Files:** `crates/spindle-host/src/host.rs:1293` (`Host::list_with`, which projects one row per instance and calls `view::order`; doc comment at 1286-1288) — cited
- **Symbols:** `view::unsupported`, `Host::list_with`, `ResolvedView::aggregation` / `ResolvedAggregation { group_by, functions }` (ess-compiler 0.90.0 `ir.rs:2509`) — cited
- **Also likely:** `view::order` (`view.rs:558`, tie-breaks on row identity, which grouped rows lack) and `view::holds` (`:455`, for each aggregate's `where:`) — inferred
- **Also likely:** `ess_conformance::aggregate::evaluate` (ess 0.90.0 `aggregate.rs:92`, `pub`), reused from `view.rs`; `ess-conformance` is already a dependency of spindle-host — inferred
- **Also likely:** a new test in `crates/spindle-cli/tests/spindle-cli/`, a fixture under `crates/spindle-cli/tests/fixtures/`, and a `mod` line in its `main.rs`, compared against `ess_conformance::interpret` as `view_field_filters.rs` does — inferred, from commit 2a5bf3a5
- **Documents:** `website/docs/concepts/specification.md:66-67` — cited
- **Confidence:** high for the surface; medium for the grouped-read path (`list_with` or a new `view.rs` function)
- **Would collide with:** units editing `view::unsupported` or `view::project`, `Host::list_with` or its doc comment, the Views paragraph of `website/docs/concepts/specification.md`, or adding a `mod` line to `crates/spindle-cli/tests/spindle-cli/main.rs`
- **Safety fact:** `view::unsupported` has one caller, `readable` (`host.rs:1731`), so lifting the aggregation arm changes `Host::view`, `Host::list`/`list_with` and `host::reads` (behind `surface.rs:149/820`) together, and no other caller — unproven
- **Not established:** what a one-row read (`Host::view`, `host.rs:1222`) of an aggregate view answers; the row order ESS expects for groups (`interpret/views.rs:399` groups through a `BTreeMap`); whether an aggregate's `where:` stays within `view::holds`; whether the test goes in `spindle-cli` or `spindle-host`; whether ingress publishes a different row schema for aggregate views.

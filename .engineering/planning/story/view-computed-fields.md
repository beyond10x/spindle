---
format: aep.planning-md/3
id: story:view-computed-fields
kind: story
status: active
title: A computed view field is served
tags:
- downstream-gap
relations:
- serves: vision:spindle
- decomposes: epic:downstream-kernel-constructs
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-host/src/view.rs
- confidence: inferred
  path: crates/spindle-host/tests/fixtures/views/board.yaml
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/views.rs
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T22:14:37Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T22:14:37Z", actor: "human:timo", revision: 3}
---
# A computed view field is served

## Outcome

A view field declared with `expr:` over the row's own fields is computed for each row. Today
computed fields stay refused (`story:views-with-filters-and-lists`). A downstream specification has
6 such views (its count).

## Acceptance

On a neutral fixture with a view field `score_change: {expr: "score - coalesce(previous_score,
score)"}`, each row carries the computed value, equal to ESS 0.90.0's interpreter, including a row
whose `previous_score` is absent.

## Scope

Derived 2026-11-10 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-host` (the view reader) — cited; `ResolvedView.computed` is read only in `crates/spindle-host/src/view.rs`
- **Files:** `crates/spindle-host/src/view.rs:49` (`unsupported`, the `"computed fields"` refusal) — cited
- **Files:** `crates/spindle-host/src/view.rs:517` (`project`, builds each row and must evaluate the `expr:`) — cited
- **Files:** `crates/spindle-host/src/host.rs:1250`, `:1319` (the calls to `project` in `Host::view` and `Host::list_with`) and the rustdoc at `:1287` — inferred; `project` cannot fail today, but ESS refuses a read whose computed value is unknown
- **Symbols:** `ResolvedView::computed`, `view::unsupported`, `view::project` — cited
- **Also likely:** `crates/spindle-host/tests/spindle-host/views.rs` and a view with `score_change: {expr: ...}` in `crates/spindle-host/tests/fixtures/views/board.yaml` or a new fixture beside it — inferred
- **Also likely:** `CHANGELOG.md` `## [Unreleased]` — inferred
- **Confidence:** high — the refusal and the row projection are in one file; ESS's behaviour is `ess-conformance` `interpret/views.rs:461-470` and `:499` at 0.90.0
- **Would collide with:** units touching the view reader's refusal chain or row projection in `view.rs`, the `Host::view` / `Host::list_with` read path in `host.rs`, or the views fixture. `view.rs:49-56` checks computed, aggregation and reference fields in one chain, so `story:view-aggregate-fields` and `story:view-related-fields` land on the same lines
- **Safety fact:** `view::unsupported` is the only gate on computed views, and `spindle-ingress` serves what `spindle_host::reads` admits (`surface.rs:962`), so lifting it serves these views over HTTP; that is safe only if every unknown or mistyped computed value refuses the read as ESS does (`views.rs:463-468`) — unproven
- **Not established:** whether computed fields read identity and state beside stored fields; whether `coalesce` and subtraction are in `ess_primitives::expr`'s evaluator; how many of the downstream's 6 views also hold another refused construct.

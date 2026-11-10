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
revision: 3
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

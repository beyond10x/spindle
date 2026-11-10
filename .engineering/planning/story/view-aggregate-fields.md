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
revision: 3
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

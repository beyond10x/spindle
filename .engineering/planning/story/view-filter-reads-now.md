---
format: aep.planning-md/3
id: story:view-filter-reads-now
kind: story
status: active
title: A view filter ordering against now minus a duration is decided
tags:
- downstream-gap
relations:
- informed_by: story:kernel-guard-reads-now
- serves: vision:spindle
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-10T04:41:20Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-10T04:41:20Z", actor: "human:timo", revision: 3}
---
## Outcome

A view filter that orders a timestamp against `now` minus a declared duration selects its rows on
the kernel's clock, as ESS's interpreter does, where the specification's format admits `now` in a
view filter.

## Why

Reported by a downstream consumer on 2026-11-10. Leases and stale-row cleanup depend on rules such
as `started_at < now - 5m`. The command-guard half of the report is story:kernel-guard-reads-now.
This story holds the view-filter half the report also names. story:kernel-guard-reads-now notes
that ESS refuses `now` in a view filter's pre-`ess/30` form. Whether a later format admits it, and
what the kernel answers today, is the first thing this story establishes.

## Acceptance

- On a fixture whose format admits `now` in a view filter, a row older than 5 minutes is returned
  by a filter `<timestamp> < now - 5m`.
- A row younger than 5 minutes is not returned.
- The query answers without 501.
- If no format ESS 0.90 implements admits `now` in a view filter, the story closes as a
  specification artifact that cites the refusal.

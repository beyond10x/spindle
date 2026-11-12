---
format: aep.planning-md/3
id: story:view-filter-reads-now
kind: story
status: implemented
title: A view filter ordering against now minus a duration is decided
tags:
- downstream-gap
relations:
- informed_by: story:kernel-guard-reads-now
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/view-field-filters
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/view_field_filters.rs
- confidence: inferred
  path: crates/spindle-docs/src/status.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-host/src/view.rs
- confidence: inferred
  path: crates/spindle-host/tests/fixtures/adversary-view-filters/slips.yaml
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/adversary_view_filters.rs
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-10T04:41:20Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-10T04:41:20Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-12T01:42:32Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"test_result":1}}}
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

## Scope

Derived 2026-11-10 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-host`, the view read path — cited. `view::reads_now` (`view.rs:397`), `Read::now` (`view.rs:386`), `view::holds` (`view.rs:456`) and `read_instant` (`host.rs:1764`, read at 1240 and 1307) already read the host's clock once per read for a filter ordered against `now`, `now - 1h` or `now + 5m` (commit 16c6b46d, in tags 0.6.0 to 0.9.0)
- **Files:** `crates/spindle-host/tests/spindle-host/adversary_view_filters.rs` — inferred, gains the older-than-5m / younger-than-5m cases; it already pins both clocks and compares against ESS's interpreter
- **Files:** `crates/spindle-host/tests/fixtures/adversary-view-filters/slips.yaml` — inferred, gains `filter: issued_at < now - 5m` (today only `issued_at < now`, :78)
- **Symbols:** `reads_now`, `read_instant`, `Read::now`, `holds` — cited
- **Also likely:** `crates/spindle-host/src/view.rs` — inferred, only if the `now - 5m` case fails
- **Also likely:** `crates/spindle-cli/tests/spindle-cli/view_field_filters.rs`, `crates/spindle-cli/tests/fixtures/view-field-filters` — inferred, the HTTP-level home for "answers without 501" (`spindle-ingress/src/answer.rs:171`, 501 mapping at :83)
- **Documents:** `crates/spindle-docs/src/status.rs` — inferred
- **Confidence:** medium — the read path is cited and already implements the feature; which test layer the acceptance means is a guess
- **Would collide with:** units touching `spindle-host`'s view read (`view.rs`, `host.rs` `list_with`/`view`/`read_instant`) or the `adversary-view-filters` fixture and test
- **Shared with `story:kernel-guard-reads-now`:** no file; that story lands in `crates/spindle-kernel`
- **Safety fact:** ESS 0.90.0 admits `now` in a view filter from `ess/30` (`ess-domain/src/expression.rs:700`) and `CurrentTime::parse` accepts `now - 5m` (`ess-primitives/src/time.rs:966`), so the existing host path decides the filter with no kernel change — unproven
- **Not established:** whether `now - 5m` already passes (no test runs it); where the downstream's 501 came from (a release below 0.6.0, a nullable field the row does not hold, or another refused construct such as paging, computed or reference fields at `view.rs:30`).

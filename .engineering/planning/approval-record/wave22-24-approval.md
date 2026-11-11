---
format: aep.planning-md/3
id: approval-record:wave22-24-approval
kind: approval-record
status: accepted
title: 'Waves 22 to 24: the 0.14.0 downstream findings, then the next active stories'
tags:
- operator
relations:
- decides: story:view-aggregate-fields
- decides: story:events-read-bounded
- decides: story:events-head-cursor
- decides: story:served-operations-route
- decides: story:ingress-bind-address
- decides: story:state-change-conditions
- decides: story:view-computed-fields
- decides: story:response-field-value-source
- decides: story:outcome-response-headers
- decides: story:view-filter-through-reference
- decides: story:view-filter-reads-now
- decides: story:upsert-creation-related-refusal-first
- decides: story:absent-optional-in-row-set-filter
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-11T20:32:27Z", actor: "human:timo", revision: 2}
---
# Waves 22 to 24: the downstream consumer's 0.14.0 findings, then the next active stories

## Who decided

The operator, in the coordinating session on 2026-11-11, after reading the five findings a
downstream consumer reported on 0.14.0 (aggregation, computed fields, filters through a
reference, the events head cursor, a route listing what is not served): "ok, all above must be
fixed, and further more, whats next, select the next 3 waves and sipatch them".

That accepts the three drafts filed from those findings (`story:events-head-cursor`,
`story:view-filter-through-reference`, `story:served-operations-route`) and orders three waves.
The operator's standing order for wave 21 still holds for each: everything of a wave in one
integration branch, cheap gates per unit, the expensive gates once at the end, one PR, auto merge,
one release.

## What it decides

- Wave 22, integration branch `wave/w22`, release 0.15.0:
  - `story:view-aggregate-fields` (43 views of the consumer, 27 of 137 pages)
  - `story:events-read-bounded` with `story:events-head-cursor`, one unit (both land on the store
    read after a position and `range.rs`)
  - `story:served-operations-route`
  - `story:ingress-bind-address`
  - `story:state-change-conditions`
- Wave 23, `wave/w23`, release 0.16.0:
  - `story:view-computed-fields` (lands on `view.rs` after aggregation)
  - `story:response-field-value-source`
  - `story:outcome-response-headers`
- Wave 24, `wave/w24`, release 0.17.0:
  - `story:view-filter-through-reference` with `story:view-filter-reads-now`, one unit (one
    filter reader in `view.rs`)
  - `story:upsert-creation-related-refusal-first`
  - `story:absent-optional-in-row-set-filter`
- Not in these waves, active and unscoped: `story:external-refusal-before-row`,
  `story:seed-at-start`, `story:ess-format-range`.

Waves follow the collisions `aep plan artifact waves --kind story --status active` printed: the
view stories share `crates/spindle-host/src/view.rs` and `host.rs`, and the kernel stories share
`crates/spindle-kernel/src/execute.rs`, so each wave holds at most one of each group.

## Decisions taken by the coordinator

- `story:ingress-bind-address`: the listen address is an explicit option and the default stays
  loopback, so no running deployment changes its exposure by upgrading.
- The ESS Decimal mismatch the consumer reported (ESS's type projection writes Decimal as a string,
  ESS's value table admits only a number) belongs to ESS, not to these waves.

---
format: aep.planning-md/3
id: review-result:w14-u4-adversary-1
kind: review-result
status: active
title: Adversary pass 1 on view-field-literal-filters (wave 14)
relations:
- reviews: story:view-field-literal-filters
revision: 1
---
Adversary pass 1 on `story:view-field-literal-filters`, wave 14 unit U4. Recorded from the
adversary's report.

Ordered comparisons on every scalar (Timestamp by instant, Decimal, Integer boundaries, enums,
Duration), `forall` over an empty list, mixed `any`/`all`/`not` with an unread leaf, nested binders,
absent optionals and the row-to-facts conversion agree with ESS's interpreter. Both findings were
fixed in the unit (16c6b46); the adversary's cases (`crates/spindle-host/tests/spindle-host/adversary_view_filters.rs`)
pass unchanged.

```findings
- file: crates/spindle-host/src/view.rs
  line: 178
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a filter ordering a field against now was admitted but the view read supplied no FactSource::now, so every read finding a row was refused as unknown; fixed by reading the host clock once per read, as ESS's interpreter does
- file: crates/spindle-host/src/host.rs
  line: 1222
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: Host::list_with's doc listed only the old filters; fixed
```

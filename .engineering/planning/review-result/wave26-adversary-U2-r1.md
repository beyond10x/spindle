---
format: aep.planning-md/3
id: review-result:wave26-adversary-U2-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 26 U2
relations:
- reviews: story:view-filter-parameter-literal
revision: 1
---
approve

```
unit: story:view-filter-parameter-literal, head 9c2bfdcc5
verdict: nothing found
cases: executed 164→171, red 0
origin: introduced 0 / pre-existing 0 / undecided 1
```

Kept: `crates/spindle-host/tests/spindle-host/adversary_w26_u2.rs`, every case green on the memory and
the SQLite store.

Note, undecided origin: a view filter that quantifies over a list member of an outer list-field
element (`exists g in slots: (exists w in g.words: w == "x")`) is refused whole, where ESS 0.99.1's
interpreter answers it. Spindle serves less there and never a different answer. Filed as the
backlog draft `story:view-filter-nested-element-quantifier`.

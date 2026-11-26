---
format: aep.planning-md/3
id: review-result:wave26-adversary-U1-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 26 U1
relations:
- reviews: story:related-selector-rows-quantifier
- reviews: story:guard-over-absent-row-field
revision: 1
---
needs-revision

```
unit: story:related-selector-rows-quantifier, story:guard-over-absent-row-field, head 59d9ce964
verdict: CONFIRMED
cases: executed 546→559, red 2
origin: introduced 1 / pre-existing 1 / undecided 0
```

Red on 59d9ce964 (`crates/spindle-kernel/tests/spindle-kernel/adversary_w26_u1.rs`): a `when_related`
row-set selector that walks an entity another component owns was served and read that entity from
the running component's store, answering `charged` where ESS 0.99.1's interpreter answers `on-hold`
(introduced; the base refused it by name). The same answer for a row set that selects another
component's entity directly (pre-existing).

Fixed in cbd792a9a: `foreign_guard(ir, command)` in `crates/spindle-kernel/src/supported.rs` refuses
a `when_related` guard over another component's rows (a row set, a walked entity, a single row with
`also:`) at admission. Both cases are green; kernel 394 passed, host 154 passed.

Reading another component's store in one host is filed as `story:cross-component-related-reads`.

```findings
- file: crates/spindle-kernel/src/supported.rs
  category: parity
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a when_related row-set selector that walks an entity another component owns was served from the running component's store, answering charged where the interpreter answers on-hold
- file: crates/spindle-kernel/src/supported.rs
  category: parity
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: a when_related row set selecting an entity another component owns was served and found no row, answering charged where the interpreter answers on-hold
```

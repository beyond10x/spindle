---
format: aep.planning-md/3
id: review-result:w21-adv-kernel
kind: review-result
status: active
title: Wave 21 kernel, adversary pass 1
relations:
- reviews: story:kernel-value-expressions
- reviews: story:kernel-secondary-effects
revision: 1
---
needs-revision

Adversary pass on wave 21's kernel merges: value expressions (b6fd783e) and secondary effects
(473f1839); cases on review/w21-adv-kernel, commit eaa6f9b3.

- Secondary effects — a deletes outcome whose affects writes are all removals, or select none
  (delete.rs:45 Store::remove), committed with no condition on the scanned entity, so a session a
  concurrent writer opened after the scan outlived its account on SQLite and PostgreSQL —
  crates/spindle-kernel/src/execute/affects.rs:264. Fixed in b72f8824; the same class in a guarded
  delete, a guarded set outcome and a set outcome's own selection fixed in b72f8824 and 333de6ba.

Attacked without a break: 38 odd inputs to value expressions against ESS 0.92.0 step by step,
lists and maps in expressions, group atomicity on one conflicting row, 10,000-row affects cost
(0.51 to 0.63 s in the kernel, debug).

```findings
- file: crates/spindle-kernel/src/execute/affects.rs
  line: 264
  category: concurrency
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a deletes outcome whose affects writes are all removals, or select none (delete.rs:45 Store::remove), commits with no condition on the scanned entity, so a row a concurrent writer adds after the scan outlives the removal on SQLite and PostgreSQL
```

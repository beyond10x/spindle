---
format: aep.planning-md/3
id: review-result:w14-u3-adversary-1
kind: review-result
status: active
title: Adversary pass 1 on kernel-row-set-guards (wave 14)
relations:
- reviews: story:kernel-row-set-guards
revision: 1
---
Adversary pass 1 on `story:kernel-row-set-guards`, wave 14 unit U3. Recorded from the adversary's
report.

`count:`, `forall:` over some and no rows, selectors on identity and held state, absent optional
fields and rows created by earlier steps agree with ESS's interpreter. F1 was fixed in the unit
(2382984): an absent row on a row-set upsert is left to the row sets and then the creation, as
ESS's `creation_takes_absent` does. F2 needs a store interface change and is filed as
`story:kernel-row-set-commit-conflict`; its case is ignored, naming that story.

```findings
- file: crates/spindle-kernel/src/execute.rs
  line: 446
  category: contract-drift
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: on a row-set upsert the kernel answered an absent addressed row as a missing instance, where ESS lets the row sets answer first and then the creation; fixed
- file: crates/spindle-kernel/src/execute.rs
  line: 704
  category: concurrency
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the commit is not conditioned on the scanned row set, so a matching row another writer commits between the scan and the commit slips past the guard; filed as story:kernel-row-set-commit-conflict
```

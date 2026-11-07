---
format: aep.planning-md/3
id: review-result:panel-w15-drafts-r2
kind: review-result
status: active
title: Five drafts, plan-critic panel, round 2
relations:
- reviews: story:kernel-row-set-commit-conflict
- reviews: story:route-literal-segment-precedence
- reviews: story:kernel-creation-takes-unknown-row
- reviews: story:kernel-guard-reads-now
- reviews: story:links-second-hello-race
revision: 1
---
needs-revision

Plan-critic panel, round 2, on the same five stories after revision a819487.

LANE: acceptance — approve (all five)
LANE: scope — approve (all five)
LANE: design — approve four; needs-revision story:kernel-row-set-commit-conflict: its Outcome said story:kernel-secondary-effects covers set-outcome and `affects:` selections, which that story does not say.
LANE: parallel-safety — approve four; needs-revision story:kernel-row-set-commit-conflict: its Scope omitted the other `impl Store` and `Change` construction sites (`crates/spindle-host/src/host.rs`, spindle-host and spindle-kernel test stores, `Change` literals in tests, the browser store's adversary tests).

```findings
- file: .engineering/planning/story/kernel-row-set-commit-conflict.md
  line: 29
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome claimed story:kernel-secondary-effects covers set-outcome and affects: selections; reword as an open gap"
- file: .engineering/planning/story/kernel-row-set-commit-conflict.md
  line: 45
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "Scope omitted the other impl Store and Change construction sites, host.rs among them"
```

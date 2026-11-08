---
format: aep.planning-md/3
id: approval-record:drafts-panel-w15
kind: approval-record
status: accepted
title: Five drafts accepted after the plan-critic panel
relations:
- decides: story:kernel-row-set-commit-conflict
- decides: story:route-literal-segment-precedence
- decides: story:kernel-creation-takes-unknown-row
- decides: story:kernel-guard-reads-now
- decides: story:links-second-hello-race
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-07T18:59:08Z", actor: "human:timo", revision: 2}
---
# Five drafts accepted after the plan-critic panel

## Words

The operator's standing acceptance (2026-11-07, evening): drafts this repository files may be
activated once they pass the plan-critic panel. The operator also accepted
story:kernel-row-set-commit-conflict for wave 15, run after story:kernel-set-outcomes, because it
removes the ignored test at `crates/spindle-kernel/tests/spindle-kernel/adversary_row_set.rs:241`.

## Evidence

- review-result:panel-w15-drafts-r1 (needs-revision), review-result:panel-w15-drafts-r2
  (needs-revision, one story), review-result:panel-w15-drafts-r3 (approve). Every lane approves
  every story of the set.

## Placement

- story:kernel-row-set-commit-conflict: wave 15, unit U3, after U2 story:kernel-set-outcomes and
  merged after U1 story:ingress-prometheus-metrics (shared `crates/spindle-host/src/host.rs`).
- story:route-literal-segment-precedence, story:kernel-creation-takes-unknown-row,
  story:kernel-guard-reads-now, story:links-second-hello-race: the next wave.

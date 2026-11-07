---
format: aep.planning-md/3
id: approval-record:wave15-approval
kind: approval-record
status: accepted
title: Wave 15 runs on the operator's standing acceptance of future waves
relations:
- decides: story:ingress-prometheus-metrics
- decides: story:kernel-set-outcomes
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-07T18:35:29Z", actor: "human:timo", revision: 2}
---
# Wave 15 runs on the operator's standing acceptance of future waves

## Words

The operator accepted every wave and the next ten future waves of this repository (hand-off of
2026-11-07: "Timo accepted all waves and the next 10 future waves"). The dispatch of 2026-11-07
(evening) says: deliver the repository's active AEP stories in waves from `aep plan artifact
waves`, full gate, one PR per wave, merge on green, release within 4 h of the merge.

## The wave

Page: `.engineering/waves/wave-15.md`. Integration branch `wave/15`, forked from `main` at 61e946c.

- U1 `story:ingress-prometheus-metrics`
- U2 `story:kernel-set-outcomes`

Wave 1 of the derived plan (`aep plan artifact waves --kind story --status active`) is U1 and U2,
with no collision between them.

## Commits it covers

The unit commits, the merges into `wave/15`, review fix commits, the release preparation commit,
the closing store commit, one PR to `main` through the bot, and a release after the PR merges.

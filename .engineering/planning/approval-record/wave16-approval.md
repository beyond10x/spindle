---
format: aep.planning-md/3
id: approval-record:wave16-approval
kind: approval-record
status: accepted
title: Wave 16 runs on the operator's standing acceptance of future waves
relations:
- decides: story:kernel-creation-takes-unknown-row
- decides: story:external-answers-observed
- decides: story:surface-cross-component-template-collision
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-08T05:24:39Z", actor: "human:timo", revision: 2, executor: "agent:spindle-controller"}
---
# Wave 16 runs on the operator's standing acceptance of future waves

## Words

The operator accepted every wave and the next ten future waves of this repository (hand-off of
2026-11-07: "Timo accepted all waves and the next 10 future waves"). Waves are delivered from
`aep plan artifact waves`, full gate, one PR per wave, merge on green, release within 4 h of the
merge.

## The wave

Page: `.engineering/waves/wave-16.md`. Integration branch `wave/16`, forked from `wave/15` at
6b5d5342 while wave 15's gate runs.

- U1 `story:kernel-creation-takes-unknown-row`
- U2 `story:external-answers-observed`
- U3 `story:surface-cross-component-template-collision`

## Commits it covers

The unit commits, the merges into `wave/16`, review fix commits, the release preparation commit,
the closing store commit, one PR to `main` through the bot, and a release after the PR merges.

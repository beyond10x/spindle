---
format: aep.planning-md/3
id: approval-record:wave18-approval
kind: approval-record
status: accepted
title: Wave 18 runs on the operator's standing acceptance of future waves
relations:
- decides: story:kernel-guard-reads-now
- decides: story:kernel-existing-instance
- decides: story:external-cause-bound-to-http-call
- decides: story:live-topology
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-10T09:01:23Z", actor: "human:timo", revision: 2}
---
# Wave 18 runs on the operator's standing acceptance of future waves

## Words

The operator accepted every wave and the next ten future waves of this repository (hand-off of
2026-11-07: "Timo accepted all waves and the next 10 future waves", recorded in
`approval-record:wave16-approval`). On 2026-11-10 the operator asked "whats next" after release
0.9.0. Waves are delivered from `aep plan artifact waves`, full gate, one PR per wave, merge on
green, release within 4 h of the merge.

`story:kernel-guard-reads-now` joins the verb's wave 1 because
`approval-record:downstream-gaps-acceptance-4` asks for it in the first wave after wave 17.

## The wave

Page: `.engineering/waves/wave-18.md`. Integration branch `wave/18`, forked from `main` at
244fb60e.

- U1 `story:kernel-guard-reads-now`
- U2 `story:kernel-existing-instance`
- U3 `story:external-cause-bound-to-http-call`
- U4 `story:live-topology`

## Commits it covers

This approval commit with the scopes and the wave page, the unit commits, the merges into
`wave/18`, review fix commits, the release preparation commit, the closing store commit, one PR to
`main` through the bot, and a release after the PR merges.

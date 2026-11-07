---
format: aep.planning-md/3
id: approval-record:wave13-approval
kind: approval-record
status: accepted
title: Wave 13 runs on the operator's standing acceptance of future waves
relations:
- decides: story:ingress-request-log
- decides: story:kernel-related-row-values
- decides: story:service-worker-serves-a-specification
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-07T04:11:58Z", actor: "human:timo", revision: 2}
---
# Wave 13 runs on the operator's standing acceptance of future waves

## Words

The operator accepted every wave and the next ten future waves of this repository, as the
hand-off of 2026-11-07 records ("Timo accepted all waves and the next 10 future waves"). The
dispatch of 2026-11-07 says: derive wave 13 from `aep plan artifact waves --kind story --status
active`, and honour the condition of `verification-report:release-0.3.0` on
`crates/spindle-store-browser`.

## The wave

Page: `.engineering/waves/wave-13.md`. Integration branch `wave/13`, forked from
`store/wave12-close` (wave 12's store close rides this wave's PR, `approval-record:wave12-close-branch`).

- U1 `story:ingress-request-log`
- U2 `story:kernel-related-row-values`
- U3 `story:service-worker-serves-a-specification`
- U4 the ESS pin from 0.88.0 to 0.89.0, a dependency move

Wave 1 of `aep plan artifact waves --kind story --status active` (aep 0.79.0) is U1 to U3, with no
collision among them. The drafts `story:browser-store-several-tabs` and
`story:links-hello-test-timing` join the wave only once accepted.

## Commits it covers

The unit commits, the merges into `wave/13`, review fix commits, the closing store commit, one PR
to `main` through the bot, and a release after the PR merges.

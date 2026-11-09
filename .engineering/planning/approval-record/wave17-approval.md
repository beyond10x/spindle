---
format: aep.planning-md/3
id: approval-record:wave17-approval
kind: approval-record
status: accepted
title: 'Wave 17: the operator names the ESS review''s high findings as one wave'
relations:
- decides: epic:spec-states-runtime
- decides: story:admission-decision-checked-by-model
- decides: story:placement-refusals-match-target
- decides: story:unknown-component-refused-at-deploy
- decides: story:environment-implementations-specified
- decides: story:store-reset-and-world-specified
- decides: story:periodic-timing-specified
- decides: story:binding-delivery-specified
- decides: story:ingress-caller-identity-specified
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-09T21:34:09Z", actor: "human:timo", revision: 2}
---
# Wave 17: the operator names the ESS review's high findings as one wave

## Words

On 2026-11-09, after the ESS review of `ess/` (`review-result:ess-review-2026-11-09`), the operator
wrote: "dispatch the 15 high findings as one single wave epic, use /aep:implementing and use
/worktree:managing-worktrees".

The review's high findings number 16 when counted (H1 to H16 in the review record). The
coordinator's earlier report said 15, and that was a miscount. All 16 are in this wave.
H3 is the outcome of the accepted `story:live-topology` and is not filed again.

The operator named these stories as the wave and told the coordinator to dispatch them. The
coordinator therefore walked each from `draft` through `proposed` to `active` on that instruction,
together with `epic:spec-states-runtime`. Under the standing acceptance in
`approval-record:wave16-approval` (ten future waves), this wave runs without a second stop.

## Commits it covers

The unit commits, the merges into `wave/17`, review fix commits, the closing store commit, and one
PR to `main` through the bot, merged when CI's `Gate` is green. No release, tag or version bump.

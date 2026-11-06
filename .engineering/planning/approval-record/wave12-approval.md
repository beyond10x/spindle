---
format: aep.planning-md/3
id: approval-record:wave12-approval
kind: approval-record
status: accepted
title: Wave 12 approved by the operator's delegate
relations:
- decides: story:outcome-scoped-refusal
- decides: story:view-list-query-parameters
- decides: story:http-outcome-bodies
- decides: story:observe-ingress-model
revision: 3
transitions:
- {from: "draft", to: "accepted", at: "2026-11-06T21:32:47Z", actor: "human:timo", revision: 2}
---
# Wave 12 approved by the operator's delegate

## Words

The orchestrator, relaying the operator's grant, 2026-11-06:

> Item: wave 12 [...]. In order:
> story outcome-scoped-refusal, story view-list-query-parameters, story http-outcome-bodies.
> Fill up to 4 units with other accepted stories whose surfaces do not collide. [...] The wave is
> approved, so do not stop for approval: write the approval-record and proceed. Take only stories
> the store shows as accepted.

## The wave

Page: `.engineering/waves/wave-12.md`. Four units on integration branch `wave/12`:

- U1 `story:outcome-scoped-refusal`
- U2 `story:view-list-query-parameters`
- U3 `story:http-outcome-bodies`, after U1 merges (`depends_on`)
- U4 `story:observe-ingress-model`, the fill: no collision with U1 to U3 in
  `aep plan artifact waves --kind story --status active` (aep 0.79.0)

U1, U2 and U3 share `crates/spindle-ingress/src/surface.rs`; U1 and U2 are split by function, U3
follows U1. `story:browser-store-several-tabs` stays a draft: the wave does not touch
`crates/spindle-store-browser`.

## Commits it covers

The unit commits, the merges into `wave/12`, adversary fix commits, the closing store commit, one
PR to `main` through the bot, and the release the item names.

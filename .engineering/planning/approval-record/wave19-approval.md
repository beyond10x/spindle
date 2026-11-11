---
format: aep.planning-md/3
id: approval-record:wave19-approval
kind: approval-record
status: accepted
title: Wave 19 runs on the operator's standing acceptance of future waves
relations:
- decides: story:postgres-store
- decides: story:http-caller-proves-actor
- decides: story:binding-where-condition
- decides: story:state-moves-to-postgres
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-10T15:52:44Z", actor: "human:timo", revision: 2}
---
# Wave 19 runs on the operator's standing acceptance of future waves

## Words

The operator accepted every wave and the next ten future waves of this repository
(`approval-record:wave16-approval`). On 2026-11-10 he accepted `story:postgres-store` and
`story:state-moves-to-postgres` for wave 19, first in it (`approval-record:postgres-wave19-acceptance`:
"A: both, wave 19"). `story:http-caller-proves-actor` was announced for wave 19 to the downstream
consumer once its design decision was accepted (`architecture-decision-record:http-caller-proof`),
and `story:binding-where-condition` was accepted by that consumer
(`approval-record:downstream-gaps-acceptance-5`).

## The wave

Page: `.engineering/waves/wave-19.md`. Integration branch `wave/19`, forked from `main` at 2d78103e
(release 0.10.0).

- U1 `story:postgres-store`
- U2 `story:http-caller-proves-actor`
- U3 `story:binding-where-condition`
- U4 `story:state-moves-to-postgres`, dispatched once U1 is merged into `wave/19`

The verb `aep plan artifact waves` places `story:postgres-store` in its wave 3 for inferred
collisions with `story:http-caller-proves-actor` and `story:ingress-bind-address` in
`crates/spindle-cli/src/{main,serve}.rs`; the wave page splits those surfaces between U1 and U2.

## Commits it covers

This approval commit with the scopes and the wave page, the unit commits, the merges into
`wave/19`, review fix commits, the closing store commit, one PR to `main` through the bot, and a
release (0.11.0) within 4 h of the merge.

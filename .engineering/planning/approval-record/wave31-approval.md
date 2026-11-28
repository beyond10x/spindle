---
format: aep.planning-md/3
id: approval-record:wave31-approval
kind: approval-record
status: accepted
title: Wave 31 runs eight production-readiness stories
relations:
- decides: story:postgres-lane-kills-slow-tests
- decides: story:spindle-image-published
- decides: story:listener-tls
- decides: story:verified-caller-token
- decides: story:spec-change-over-stored-rows
- decides: story:several-writers-one-postgres
- decides: story:event-egress-webhook
- decides: story:dead-letters-kept-and-replayed
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-27T13:07:32Z", actor: "human:operator", revision: 2}
---
## Words

The host operator, acting with the operator's authority, 2026-11-27, relaying the operator's order
"make sure spindle is moving with its last planned features, must continue to be developed". The
host operator answered the wave 31 decisions at 12:10 CET: D-W31-JWT A, D-W31-SPLIT A,
D-W31-WRITERS A, D-W31-SPEC A, D-W31-IMAGE A, "The check:postgres 60 s budget unit first: agreed.
Proceed." D3 of `story:verified-caller-token` was taken as B on the coordinator's default.

Each story passed the four plan critics (`review-result:wave31-*-r1` to `-r2`, and
`review-result:wave31-acc-r3` to `-r4` for `story:several-writers-one-postgres`) before its move
to `active`. None changes the architecture under the options the host operator picked.

## The wave

Page: `.engineering/waves/wave-31.md`. Integration branch `wave/31`, forked from `wave/30` after
wave 30's units merge.

- U1 `story:postgres-lane-kills-slow-tests`
- U2 `story:spindle-image-published`
- U3 `story:listener-tls`
- U4 `story:verified-caller-token`
- U5 `story:spec-change-over-stored-rows`
- U6 `story:several-writers-one-postgres`
- U7 `story:event-egress-webhook`
- U8 `story:dead-letters-kept-and-replayed`

Merge order: U1, U2, U3, U5, U7, U4, U8, U6.

## Commits it covers

The unit commits, the merges into `wave/31`, review fix commits, the closing store commit, one PR to
`main` through the bot, and release 0.26.0 on the host operator's word after the PR merges.

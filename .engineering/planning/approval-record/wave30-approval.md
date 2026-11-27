---
format: aep.planning-md/3
id: approval-record:wave30-approval
kind: approval-record
status: accepted
title: Wave 30 runs twelve parity and correctness stories
relations:
- decides: story:view-rows-absent-optional-fields
- decides: story:view-filter-nested-element-quantifier
- decides: story:view-filter-map-value-order
- decides: story:error-payload-generated-fields
- decides: story:unknown-instance-unsourced-unserved
- decides: story:create-on-held-identity-unavailable
- decides: story:decimal-identity-any-spelling
- decides: story:look-ahead-asks-only-where-it-matters
- decides: story:related-via-nested-input
- decides: story:events-cursor-across-restart
- decides: story:peer-connect-test-steady
- decides: story:rally-example-reads-stdin
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-27T09:51:58Z", actor: "human:operator", revision: 2}
---
## Words

The host operator, acting with the operator's authority, 2026-11-27, relaying the operator's order
"make sure spindle is moving with its last planned features, must continue to be developed". The
host operator chose the story list (D-W30 A). The wave 30 proposal went to the host operator at
10:21 CET with approval as its default. The host operator answered at 10:51 CET: "D-W30-GO = A".

Each story passed the four plan critics (`review-result:wave30-*-r1` to `-r3`) before its move to
`active`. None changes the architecture under the options taken when scoped.

## The wave

Page: `.engineering/waves/wave-30.md`. Integration branch `wave/30`, forked from `main` after wave
29's pull request merges.

- U1 `story:view-filter-nested-element-quantifier`, then `story:view-filter-map-value-order`
- U2 `story:view-rows-absent-optional-fields`
- U3 `story:unknown-instance-unsourced-unserved`, then `story:error-payload-generated-fields`
- U4 `story:look-ahead-asks-only-where-it-matters`
- U5 `story:create-on-held-identity-unavailable`
- U6 `story:decimal-identity-any-spelling`
- U7 `story:events-cursor-across-restart`
- U8 `story:peer-connect-test-steady`
- U9 `story:rally-example-reads-stdin`
- U10 `story:related-via-nested-input`

Merge order: U10, U8, U7, U1, U2, U4, U3, U5, U6, U9.

## Commits it covers

The unit commits, the merges into `wave/30`, review fix commits, the closing store commit, one PR to
`main` through the bot, and release 0.25.0 on the host operator's word after the PR merges.

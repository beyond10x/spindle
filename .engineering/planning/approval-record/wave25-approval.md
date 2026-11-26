---
format: aep.planning-md/3
id: approval-record:wave25-approval
kind: approval-record
status: accepted
title: Wave 25 runs on the operator's order for bigger waves
relations:
- decides: story:planning-only-prs-skip-tests
- decides: story:test-step-within-budget
- decides: story:wasm-lanes-within-budget
- decides: story:decisions-rules-on-input-predicates
- decides: story:guard-over-absent-optional-input
- decides: story:identity-mint-types-and-fallback
- decides: story:reset-reapplies-seed
- decides: story:world-row-integer-fields
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-26T04:51:42Z", actor: "human:operator", revision: 2}
---
# Wave 25 runs on the operator's order for bigger waves

## Words

The host operator, acting with the operator's authority, 2026-11-26 about 05:18 CET, relaying the
operator's words typed in another session: "now, move in way bigger /aep:wave and deliver PRs with
more stories + gap-closures at once", and his order of 00:20 CET: "do not submit/merge/integrate
every single bit one by one, just run bigger /aep:wave at once". The relay adds: plan the next wave
with several stories and the open gap closures together, one integration branch, one PR, one head
line when green.

The stories of this wave were named in the host operator's standing story order for this
repository (after 0.20.1: planning-only pull requests skip the tests, decision rules on input
predicates with a reset that carries decisions) and are the open consumer gaps and test-budget
defects of the store. Each passed the four plan critics (`review-result:wave25-*-r1` to `-r4`)
before its move to `active`.

## The wave

Page: `.engineering/waves/wave-25.md`. Integration branch `wave/25`, forked from `main`.

- U1 `story:planning-only-prs-skip-tests`, then `story:test-step-within-budget`, then
  `story:wasm-lanes-within-budget`, on one branch
- U2 `story:decisions-rules-on-input-predicates`
- U3 `story:identity-mint-types-and-fallback`
- U4 `story:guard-over-absent-optional-input`
- U5 `story:reset-reapplies-seed`
- U6 `story:world-row-integer-fields`

## Commits it covers

The unit commits, the merges into `wave/25`, review fix commits, the release preparation commit,
the closing store commit, one PR to `main` through the bot, and a release after the PR merges.

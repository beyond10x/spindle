---
format: aep.planning-md/3
id: approval-record:wave34-approval
kind: approval-record
status: accepted
title: 'Wave 34 runs three stories: signed release artifacts, an unknown commit answered as unknown, a reset on a busy store keeps the store'
relations:
- decides: story:release-artifacts-signed
- decides: story:unknown-commit-answered-as-unknown
- decides: story:reset-on-a-busy-store-keeps-the-store
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-28T14:14:06Z", actor: "human:operator", revision: 2}
---
## Words

The host operator, acting with the operator's authority, 2026-11-28 14:55 CET, relaying the
operator's order "make sure spindle is moving with its last planned features, must continue to be
developed": D-W34 = A, `story:release-artifacts-signed` plus the two drafts wave 33's adversaries
filed, accepted. The operator's own yes of 11:08 CET lets `story:release-artifacts-signed` add the
workflow permissions `id-token: write` and `attestations: write` to `release.yml` (cosign keyless
signing plus GitHub provenance), on jobs that run on no pull request.

Each story passed the four plan critics (`review-result:wave34-*-r1`, `wave34-acc-r2`,
`wave34-des-r2`, `wave34-par-r2`, `wave34-acc-r3`) before its move to `active`. Every scoping
decision takes the scoper's default; none changes the architecture.

## The wave

Page: `.engineering/waves/wave-34.md`. Integration branch `wave/34`, forked from `main` after wave 33
(PR 107) lands.

- U1 `story:release-artifacts-signed`
- U2 `story:unknown-commit-answered-as-unknown`
- U3 `story:reset-on-a-busy-store-keeps-the-store`

Built in parallel; merged U1, U2, U3.

## Commits it covers

The unit commits, the merges into `wave/34`, review fix commits, the closing store commit, one PR to
`main` through the bot, and release 0.29.0 after the PR merges.

---
format: aep.planning-md/3
id: approval-record:wave32-approval
kind: approval-record
status: accepted
title: 'Wave 32 runs four stories: local PostgreSQL lane, awaited system steps, prompt exit on SIGTERM, seed under a lease'
relations:
- decides: story:postgres-lane-runs-locally
- decides: story:conform-awaits-system-steps
- decides: story:leader-exits-promptly-on-sigterm
- decides: story:seed-under-a-lease
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-28T04:16:52Z", actor: "human:operator", revision: 2}
---
## Words

The host operator, acting with the operator's authority, 2026-11-28, relaying the operator's order
"make sure spindle is moving with its last planned features, must continue to be developed". The
host operator answered D-W32 with A: two consumer asks plus `story:seed-under-a-lease`,
`story:leader-exits-promptly-on-sigterm` and `story:postgres-lane-runs-locally`, with the test-time
budget fix first as its own integration PR. One consumer ask, `story:view-filter-absent-optional`,
was rejected on ESS's answer to question VF-1 (B: the refusal is ESS's rule).

Each story passed the four plan critics (`review-result:wave32-*-r1` to `-r3`, and
`review-result:wave32-acc-r4`) before its move to `active`. Every scoping decision takes the
scoper's default; none changes the architecture.

## The wave

Page: `.engineering/waves/wave-32.md`. Integration branch `wave/32`, forked from `main` after
`integration/2026-11-28-1` lands there.

- U1 `story:postgres-lane-runs-locally`
- U2 `story:conform-awaits-system-steps`
- U3 `story:leader-exits-promptly-on-sigterm`
- U4 `story:seed-under-a-lease`

Merge order: U1 and U2 in parallel, then U3, then U4.

## Commits it covers

The unit commits, the merges into `wave/32`, review fix commits, the closing store commit, one PR to
`main` through the bot, and release 0.27.0 after the PR merges.

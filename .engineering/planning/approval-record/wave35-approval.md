---
format: aep.planning-md/3
id: approval-record:wave35-approval
kind: approval-record
status: accepted
title: 'Wave 35 runs three stories: a link bound setting, the observe label vocabulary, an unknown commit in seeds and retries'
relations:
- decides: story:link-bound-setting
- decides: story:observe-label-vocabulary
- decides: story:unknown-commit-in-seed-and-retries
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-28T18:22:04Z", actor: "human:operator", revision: 2}
---
## Words

The host operator, acting with the operator's authority, 2026-11-28, relaying the operator's order
"make sure spindle is moving with its last planned features, must continue to be developed": D-W35 =
A, four stories accepted. The same message gave the coordinator a standing rule: "wave scope from
accepted epics with budget fixes first is yours to decide; report it as decided."

Under that rule the coordinator took `story:postgres-lane-margin-ci` out of the wave: its cut lands
in eventlog, another repository, and it waits for an eventlog release. The three stories below
remain.

Each story passed the four plan critics (`review-result:wave35-acc-r1`, `wave35-des-r1`,
`wave35-par-r1`, `wave35-sco-r1`, `wave35-acc-r2`, `wave35-des-r2`, `wave35-par-r2`,
`wave35-acc-r3`) before its move to `active`. Every scoping decision takes the scoper's default
(option A); none changes the architecture.

## The wave

Page: `.engineering/waves/wave-35.md`. Integration branch `wave/35`, forked from `main` after wave 34
(PR 110) lands.

- U1 `story:link-bound-setting`
- U2 `story:observe-label-vocabulary`
- U3 `story:unknown-commit-in-seed-and-retries` (depends on U1 and U2)

Built in parallel; merged U1, U2, U3. U3 regenerates the shared generated files on top of U2's
merged change and the CLI reference on top of U1's.

## Commits it covers

The unit commits, the merges into `wave/35`, review fix commits, the closing store commit, one PR to
`main` through the bot, and release 0.30.0 after the PR merges.

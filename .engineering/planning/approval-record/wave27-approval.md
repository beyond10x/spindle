---
format: aep.planning-md/3
id: approval-record:wave27-approval
kind: approval-record
status: accepted
title: Wave 27 runs on the operator's order to resolve all missing features
relations:
- decides: story:stdin-run-fires-bindings
- decides: story:stdin-arrange-and-line-references
- decides: story:seed-once-on-durable-state
- decides: story:serve-loop-signal-exit-answers
- decides: story:query-input-beside-body
- decides: story:caller-attributes-on-loopback
- decides: story:map-input-guard-facts
- decides: story:integer-identity-any-spelling
- decides: story:wasm-lanes-within-budget
- decides: story:refused-bound-command-recorded
- decides: story:cross-component-related-reads
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-26T21:00:26Z", actor: "human:operator", revision: 2}
---
## Words

The host operator, acting with the operator's authority, 2026-11-26 evening, relaying the
operator's words typed in its session: "resolve all missing spindle features". The wave 27 proposal
went to the host operator at 21:45 CET with go as its default. No answer came within 15 minutes; the default go was taken at 22:00 CET under the rule that a default proceeds.

The stories of this wave are the remaining known gaps after wave 26: consumer reports confirmed with
a reproduction or a cited line, findings of the wave 25 and wave 26 adversaries, and the wasm lane
budget. Each passed the four plan critics (`review-result:wave27-*-r1` to `-r11`) before its move to
`active`. The decision of `story:cross-component-related-reads` (D1 A) is the operator's answer,
relayed by the host operator.

## The wave

Page: `.engineering/waves/wave-27.md`. Integration branch `wave/27`, forked from `plan/wave-27` with
`wave/26` merged in.

- U1 `story:stdin-run-fires-bindings`, then `story:stdin-arrange-and-line-references`
- U2 `story:seed-once-on-durable-state`, then `story:serve-loop-signal-exit-answers`
- U3 `story:query-input-beside-body`, then `story:caller-attributes-on-loopback`
- U4 `story:map-input-guard-facts`
- U5 `story:integer-identity-any-spelling`
- U6 `story:wasm-lanes-within-budget`
- U7 `story:refused-bound-command-recorded`
- U8 `story:cross-component-related-reads`

Merge order: U6, U4, U5, U2, U1, U3, U7, U8.

## Commits it covers

The unit commits, the merges into `wave/27`, review fix commits, the closing store commit, one PR to
`main` through the bot, and a release on the host operator's word after the PR merges.

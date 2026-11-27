---
format: aep.planning-md/3
id: approval-record:wave30-u11-approval
kind: approval-record
status: accepted
title: Wave 30 takes the consumer gap of affects over an input list as U11
relations:
- decides: story:affects-over-input-list
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-27T10:34:38Z", actor: "human:operator", revision: 2}
---
## Words

The host operator, acting with the operator's authority, 2026-11-27. A consumer reported at about
11:10 CET that Spindle refuses an `affects:` entry over an input list. The question which wave takes
it went to the host operator at 11:17 CET with U11 of wave 30 as its default; the host operator
answered "D-AFFECTS = A (wave 30 U11, after scope and critics; ships in 0.25.0)".

The story passed the four plan critics (`review-result:wave30-u11-*-r1` to `-r4`) before its move
to `active`. It changes no architecture under the options taken when scoped.

## The unit

U11 `story:affects-over-input-list`, branch `unit/w30-affects-each`, on the wave page
`.engineering/waves/wave-30.md`. It merges after U5 and U6, with `wave/30` merged into its branch
before its last run, and before U9.

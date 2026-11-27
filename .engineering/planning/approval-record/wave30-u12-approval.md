---
format: aep.planning-md/3
id: approval-record:wave30-u12-approval
kind: approval-record
status: accepted
title: Wave 30 takes the consumer gap of a served run stepping its clock as U12
relations:
- decides: story:served-clock-steps
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-27T10:45:05Z", actor: "human:operator", revision: 2}
---
## Words

The host operator, acting with the operator's authority, 2026-11-27, passed on a consumer's ask
that a served run step its clock, with the order to file it, scope it and propose its wave, taking
the coordinator's default. The coordinator's default, reported to the host operator: wave 30 takes
it as U12 (D-CLOCK A), since wave 30 had not started building.

The story passed the four plan critics (`review-result:wave30-u12-*-r1` to `-r3`) before its move
to `active`. It changes no architecture under the options taken when scoped.

## The unit

U12 `story:served-clock-steps`, branch `unit/w30-clock`, on the wave page
`.engineering/waves/wave-30.md`. It merges last: after U6 (`world.rs`), U7 (`range`, `depends_on`)
and U9 (`cli.rs`).

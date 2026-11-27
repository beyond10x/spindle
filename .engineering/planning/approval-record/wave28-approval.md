---
format: aep.planning-md/3
id: approval-record:wave28-approval
kind: approval-record
status: accepted
title: Wave 28 runs on the operator's order to resolve all missing features
relations:
- decides: story:console-routes-off
- decides: story:structured-json-logs
- decides: story:metrics-series-bounded
- decides: story:seed-readiness
- decides: story:health-and-graceful-drain
- decides: story:entity-event-history
- decides: story:events-record-their-caller
- decides: story:environment-binds-external-decisions
- decides: story:replay-with-decisions
- decides: story:one-time-response-admission
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-27T07:28:33Z", actor: "human:operator", revision: 2}
---
## Words

The host operator, acting with the operator's authority, 2026-11-27, relaying the operator's order
"resolve all missing spindle features". The wave 28 proposal went to the host operator at 08:26 CET
with go as its default. The host operator answered at 08:28 CET: "Wave 28: go, including health D2 B
and D4 A."

The stories of this wave are the consumer-reported gaps first in the triage of 2026-11-27, which
the host operator accepted for the operator the same morning. Each passed the four plan critics
(`review-result:wave28-*-r1` to `-r3`) before its move to `active`. Seven stories that would change
the architecture are not in this wave; they wait for the operator's picks.

## The wave

Page: `.engineering/waves/wave-28.md`. Integration branch `wave/28`, forked from `plan/wave-28`.

- U1 `story:console-routes-off`
- U2 `story:structured-json-logs`
- U3 `story:metrics-series-bounded`
- U4 `story:seed-readiness`, then `story:health-and-graceful-drain`
- U5 `story:events-record-their-caller`, then `story:entity-event-history`
- U6 `story:environment-binds-external-decisions`
- U7 `story:one-time-response-admission`, then `story:replay-with-decisions`

Merge order: U7, U6, U5, U3, U1, U4, U2.

## Commits it covers

The unit commits, the merges into `wave/28`, review fix commits, the closing store commit, one PR to
`main` through the bot, and a release on the host operator's word after the PR merges.

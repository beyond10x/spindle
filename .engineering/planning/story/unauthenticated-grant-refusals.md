---
format: aep.planning-md/3
id: story:unauthenticated-grant-refusals
kind: story
status: implemented
title: Without a fixed caller, granted operations are refused as ESS refuses them
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-11T09:40:08Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-11T09:40:09Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-11T20:31:02Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
# Without a fixed caller, granted operations are refused as ESS refuses them

## Outcome

A run serving HTTP without `--fixed-actor` answers a read-granted view, and a command whose grant
names actors, with ESS's standard 403 "not granted" (`actor: null`), as ESS's served surface does
for a request that proves no actor. Today such views are served to anyone and such commands run
with no actor (`crates/spindle-ingress/src/answer.rs`).

## Acceptance

- `a_read_granted_view_without_a_caller_is_refused_403` — nothing is read.
- `a_granted_command_without_a_caller_is_refused_403` — nothing is written.
- An ungranted view and command are served as today.

## Origin

Pre-existing. Found by the adversary of `story:served-fixed-caller` (wave 8). Whether an HTTP
caller can prove an actor stays undecided (design §11); this story only refuses as ESS does.

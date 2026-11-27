---
format: aep.planning-md/3
id: story:readiness-test-steady-under-load
kind: story
status: draft
title: The seed readiness test passes on a loaded workstation
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

`seed::readiness::a_readiness_route_answers_200_only_after_the_seed_ends` passes on a loaded
workstation: it waits for the seed by an observable state, not a fixed time.

## Why

Found during wave 30's U8 correction round: it failed once in a full spindle-cli run with
`{"ready":false,"seed":"running"}` and passed 5 of 5 alone. It does not touch peer links.

Cause, found by wave 31's U7 adversary: `crates/spindle-cli/src/seed.rs:669-673` writes the seed's
end line before it publishes the state, so the test can read `running` after the end line (3
`seed::readiness` cases red in one of two loaded spindle-cli runs).

## Acceptance

- The test passes 20 runs in a row at a 1-minute load of at least 40, and still fails when the
  readiness route answers `200` before the seed ends.

---
format: aep.planning-md/3
id: story:node-export-events
kind: story
status: draft
title: The Node export delivers published events to its host program
relations:
- serves: vision:spindle
- depends_on: story:node-export-serves-a-specification
- decomposes: epic:node-export
- depends_on: story:node-export-clock-and-seeds
scope:
- confidence: inferred
  path: crates/spindle-node
revision: 3
---
# The Node export delivers published events to its host program

## Outcome

A consumer receives every event an instance publishes, in publication order, either by a callback
it registers or by pulling the events after a cursor, as `GET /api/events` pages them
(`spindle_host::Reader`). Nothing is delivered twice to one subscriber, and a reset ends what was
delivered before it.

## Acceptance

- `a pull after a cursor returns each event once, in order` — three commands, pulls after each
  cursor, the same events `GET /api/events` lists.
- `a registered callback is called once per event, after the request that published it`.
- `a disposed instance calls no callback` — a command handed over before `dispose()` resolves,
  and no callback is called after `dispose()` returns.
- `a reset ends what was delivered` — after `reset()`, a pull from the cursor held before it
  returns no event published before the reset, and the next event published is delivered.

## Depends on

- `story:node-export-serves-a-specification`, and `story:node-export-clock-and-seeds`: both land
  on `crates/spindle-node/src/lib.rs` and its tests, and delivery hooks into the host-driving path
  that story adds.

## Scope

- `crates/spindle-node/` (inferred).

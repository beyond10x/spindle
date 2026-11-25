---
format: aep.planning-md/3
id: story:node-export-events
kind: story
status: draft
title: The Node export delivers published events to its host program
relations:
- serves: vision:spindle
- decomposes: epic:browser-host
- depends_on: story:node-export-serves-a-specification
revision: 1
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
- `a registered callback is called once per event, after the request that published it` — and not
  after `dispose()`.

## Depends on

- `story:node-export-serves-a-specification`.

## Scope

- `crates/spindle-node/` (inferred).

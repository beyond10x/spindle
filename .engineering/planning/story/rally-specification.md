---
format: aep.planning-md/3
id: story:rally-specification
kind: story
status: draft
title: The rally example system in ESS
relations:
- decomposes: epic:m1-m3-rally-in-three-vms
revision: 1
---
# The rally example system in ESS

## Outcome

`examples/rally/ess/` specifies an original two-player paddle-and-ball game (design §13): a table
component owning the match (ball, paddles, score, winner) and two player components, each deciding
by rule how to move its paddle. They communicate only through bindings: a periodic `Tick`,
`BallMoved` to each player, each player's `PaddleMoveRequested` to the table.

## Acceptance

- `ess specify validate --path examples/rally/ess` is valid, and `task check` runs it.
- `b10x-spindle capabilities examples/rally/ess --component rally-table` lists `timer`, `clock`,
  `channel` and `store.durable`; each player lists `channel` and `store.durable`.

## Scope

`examples/rally/`.

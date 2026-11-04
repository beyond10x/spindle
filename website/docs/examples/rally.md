---
title: Rally
sidebar_position: 1
description: The rally example, a two-player paddle-and-ball game written as an ESS system of three components that talk only through bindings.
lede: Three components, one periodic binding and four event bindings play a match to a winner, with no game code outside the specification.
source: examples/rally/ess, crates/spindle-host/tests/rally.rs, crates/spindle-host/src/rally.rs, crates/spindle-cli/tests/play.rs, docs/design/spindle.md §13
---

Rally is an original two-player paddle-and-ball game, written as an ESS system in
`examples/rally/ess`. The grid is 16 columns wide and 9 rows high; the west paddle stands in column
0, the east paddle in column 15, and the ball moves one cell across and one cell up or down per
tick. Every value change is a guard plus an increment on an integer field, so the game needs no
arithmetic beyond what ESS has.

## Three components

| Component | Owns | Accepts | Publishes |
|---|---|---|---|
| `rally-table` | `rally.table`: the match, ball, paddles, score, winner | `OpenMatch`, `Tick`, `MovePaddle` | `BallMoved`, `PointScored`, `MatchWon`, `PaddleMoved` |
| `rally-player-west` | `rally.west`: one player and its view of the ball | `ObserveBall` | `PaddleMoveRequested`, `BallSeen` |
| `rally-player-east` | `rally.east`: the same for the other side | `ObserveBall` | `PaddleMoveRequested`, `BallSeen` |

On each `ObserveBall` a player asks to move its paddle up if the ball is above it, down if below,
and holds when level.

## Five bindings

| Binding | Cause | Invokes |
|---|---|---|
| `tick` | periodic, every second (`PT1S`), hosted by `rally-table` | `rally.table.Tick` |
| `west-watches` | `rally.table.BallMoved` | `rally.west.ObserveBall` |
| `east-watches` | `rally.table.BallMoved` | `rally.east.ObserveBall` |
| `west-moves` | `rally.west.PaddleMoveRequested` | `rally.table.MovePaddle` |
| `east-moves` | `rally.east.PaddleMoveRequested` | `rally.table.MovePaddle` |

All are `at_most_once` with `on_failure: drop`. From these declarations Spindle derives what the
system needs: `store.durable` for the three entities, `timer` and `clock` for `tick`, `channel` for
the four event bindings between components, and `ingress.network` because the table is
`reached_by: network` ([getting started](/docs/getting-started) shows the output).

## Play a match

```console
$ b10x-spindle play examples/rally/ess --environment examples/environments/laptop.toml
```

`play` admits each component on its own against the environment, opens a match
(`--target-score`, default 3; `--match-id`, default `rally`), starts the `tick` binding and redraws
the grid from the table's `MatchState` view after every tick. `]` and `[` are the paddles and `o`
is the ball. It exits 0 once a side has won, 3 when admission refuses a component, and 130 or 143
on SIGINT or SIGTERM. [Getting started](/docs/getting-started#play-it) has the first and last
frames of a real run.

## Drive it by hand

`run` admits the whole system and executes commands read as JSON lines on stdin, one JSON result
per line. It runs commands only: no timer, no channel, so bindings do not fire.

`requests.jsonl` opens a match, ticks once, and opens the same match again:

```json
{"command": "rally.table.OpenMatch", "input": {"match_id": "m1", "target_score": 2}}
{"command": "rally.table.Tick", "input": {"match_id": "m1"}}
{"command": "rally.table.OpenMatch", "input": {"match_id": "m1", "target_score": 2}}
```

```console
$ b10x-spindle run examples/rally/ess --environment examples/environments/laptop.toml --state rally.sqlite < requests.jsonl
{"command":"rally.table.OpenMatch","events":[{"event":"rally.table.BallMoved","payload":{"ball_x":8,"ball_y":4,"east_paddle_y":4,"heading_x":"West","heading_y":"Up","match_id":"m1","west_paddle_y":4}}],"outcome":"opened"}
{"command":"rally.table.Tick","events":[{"event":"rally.table.BallMoved","payload":{"ball_x":8,"ball_y":4,"east_paddle_y":4,"heading_x":"West","heading_y":"Up","match_id":"m1","west_paddle_y":4}}],"outcome":"served"}
{"command":"rally.table.OpenMatch","failure":{"kind":"undeclared","message":"rally.table.OpenMatch: a `rally.table.Match` already has the identity \"m1\", and `creates:` never replaces an instance"}}
```

The third line is a failure, not a declared outcome: `OpenMatch` declares no outcome for a match
that already exists, and `creates:` never replaces an instance.

`store.durable` is bound to `sqlite` on the laptop environment, so `run` needs `--state`.

:::caution[Planned]
Rally as three `b10x-spindle run` processes joined by `tcp` channels, and each component in its
own microVM, are planned. The design's showcase is the second: one deployment to the environment
`microvm` (`channel = tcp`), its three components placed on three microVM targets, with admission
the only step that differs from the laptop. See [status](/docs/status).
:::

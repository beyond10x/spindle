---
title: Rally
sidebar_position: 1
description: The rally example, a two-player paddle-and-ball game written as an ESS system of three components that talk only through bindings.
lede: Three components, one periodic binding and four event bindings play a match to a winner, with no game code outside the specification.
source: examples/rally/ess, crates/spindle-host/tests/spindle-host/rally.rs, crates/spindle-host/src/rally.rs, crates/spindle-cli/tests/spindle-cli/play.rs, crates/spindle-cli/tests/spindle-cli/processes.rs, crates/spindle-cli/tests/spindle-cli/adversary_processes.rs, crates/spindle-host/src/host/component.rs, docs/design/spindle.md §13
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

## Three processes

`play --component <name>` plays one component in its own process, admitted on its own, its
bindings carried over TCP. `examples/environments/laptop-processes.toml` is `laptop.toml` with
`channel = ["tcp"]` and nothing else changed. The process launcher starts each instance with
`SPINDLE_COMPONENT`, `SPINDLE_LISTEN` and one `SPINDLE_PEER_<COMPONENT>` per other instance, and
`play` reads those, so the same command line serves all three:

```console
$ SPINDLE_COMPONENT=rally-player-west SPINDLE_LISTEN=127.0.0.1:7101 SPINDLE_PEER_RALLY_TABLE=127.0.0.1:7100 \
    b10x-spindle play examples/rally/ess --environment examples/environments/laptop-processes.toml
```

Each process listens, connects to every component its events lead to and waits for a link from
every component whose events lead to it, up to 30 s in all. A link counts only once its first
message names both ends, so a stray connection changes nothing. A component whose events lead to a
peer with no `SPINDLE_PEER_<COMPONENT>` exits 1 before it runs anything, naming each such peer and
variable. Each prints one JSON line per command it runs. Everything a tick leads to runs on every
process before the next tick, so three processes play the match one process plays. The table opens
the match, runs the tick, prints `{"view": …}`, the `MatchState` row, after each and, once a side
has won, tells both players the match is over; all three exit 0. Only the table may say so. A link
that closes before that, or a peer that breaks the link protocol, exits 1 naming the peer, and SIGINT or SIGTERM exits 130 or 143. With `SPINDLE_TELEMETRY` set, each sends its own counters as
`spindle-telemetry/1` frames. `--speed` runs the match clock that many times faster than real time.

:::caution[Planned]
Each component in its own microVM is planned. It is the design's showcase: one deployment to the
environment `microvm` (`channel = tcp`), its three components placed on three microVM targets,
with admission the only step that differs from the laptop. See [status](/docs/status).
:::

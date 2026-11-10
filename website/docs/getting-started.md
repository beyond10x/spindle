---
title: Getting started
kicker: Start
sidebar_position: 2
description: Build b10x-spindle from source, then compile, derive, admit and play the rally example.
lede: Build the binary, then take the rally example through check, capabilities, admit and play.
source: the build was run on 2026-11-10 in a checkout of 0.9.0 (main at 244fb60e); every other command was run on 2026-11-10 with the 0.10.0 release binary (tag 0.10.0 at 2d78103e) in a checkout of 0.10.0; output pasted from those runs
---

Binaries for Linux and macOS, x86_64 and aarch64 each, are attached to each
[GitHub release](https://github.com/beyond10x/spindle/releases) with a `SHA256SUMS` file. This page
builds `b10x-spindle` from source.

## Build

You need Git and Rust. The repository pins its toolchain in `rust-toolchain.toml` (Rust 1.98.1),
and `rustup` picks it up.

```console
git clone https://github.com/beyond10x/spindle
cd spindle
cargo build --locked --release -p spindle-cli
```

The last lines of the build:

```text
   Compiling spindle-conformance v0.9.0 (…/spindle/crates/spindle-conformance)
    Finished `release` profile [optimized] target(s) in 88m 23s
```

The binary is `target/release/b10x-spindle`, and the commands below call it `b10x-spindle`: put
`target/release` on your `PATH` or type the path.

```console
$ b10x-spindle --version
b10x-spindle 0.10.0
```

## Compile the specification

`examples/rally/ess` is a two-player paddle-and-ball game written as an ESS system of three
components ([the rally example](/docs/examples/rally)). `check` compiles it with `ess-compiler` and
prints what it declares:

```console
$ b10x-spindle check examples/rally/ess
rally v1 compiled from 5 file(s): 3 entities, 5 commands, 8 events, 3 views, 5 bindings, 3 components
```

## Derive its capabilities

`capabilities` prints every capability the specification needs, and under each one the
declaration that needs it:

```console
$ b10x-spindle capabilities examples/rally/ess
channel
  binding east-moves: rally.east.PaddleMoveRequested crosses to rally.table.MovePaddle in another component
  binding east-watches: rally.table.BallMoved crosses to rally.east.ObserveBall in another component
  binding west-moves: rally.west.PaddleMoveRequested crosses to rally.table.MovePaddle in another component
  binding west-watches: rally.table.BallMoved crosses to rally.west.ObserveBall in another component
clock
  binding tick: a periodic cause is placed on the clock
ingress.network
  component rally-table: reached_by: network
store.durable
  entity rally.east.Player: its instances outlive the command that changed them
  entity rally.table.Match: its instances outlive the command that changed them
  entity rally.west.Player: its instances outlive the command that changed them
timer
  binding tick: a periodic cause invokes rally.table.Tick
```

The list is derived from the entities, bindings and components in the specification
([capabilities](/docs/concepts/capabilities)). `--component` narrows it to what one component needs
on its own:

```console
$ b10x-spindle capabilities examples/rally/ess --component rally-player-west
channel
  binding west-moves: rally.west.PaddleMoveRequested crosses to rally.table.MovePaddle in another component
  binding west-watches: rally.table.BallMoved crosses to rally.west.ObserveBall in another component
store.durable
  entity rally.west.Player: its instances outlive the command that changed them
```

## Admit it against an environment

An environment document says which implementations provide which capabilities, most preferred
first. `examples/environments/laptop.toml` is one:

```toml
format = "spindle-environment/1"
name = "laptop"
os = "linux"
arch = "x86_64"

[provides]
"store.durable" = ["sqlite", "memory"]
"store.ephemeral" = ["memory"]
"identity.mint" = ["uuid-v4"]
clock = ["system"]
timer = ["tokio"]
channel = ["in-process"]
projection = ["in-process"]
"ingress.network" = ["http-loopback"]
"ingress.command_line" = ["terminal"]
"external.*" = ["operator-prompt"]
```

`admit` binds one implementation to every required capability:

```console
$ b10x-spindle admit examples/rally/ess --environment examples/environments/laptop.toml
admitted on laptop
  channel = in-process
  clock = system
  ingress.network = http-loopback
  store.durable = sqlite
  timer = tokio
```

Admission can refuse. Spindle's own specification (`ess/`) has six outcomes decided outside the
model, and the `microvm` environment provides nothing for them:

```console
$ b10x-spindle admit ess --environment examples/environments/microvm.toml
refused on microvm: no implementation for
  external.spindle.deployment.DecideAdmission.refused
  external.spindle.placement.PlaceInstance.above-ceiling
  external.spindle.placement.PlaceInstance.environment-mismatch
  external.spindle.placement.PlaceInstance.not-admitted
  external.spindle.placement.PlaceInstance.target-retired
  external.spindle.placement.PlanInstance.not-admitted
$ echo $?
3
```

Exit status 3 means refused ([environment and admission](/docs/concepts/environment-and-admission)).

## Play it

`play` runs all three rally components in one process. Each is admitted on its own, the timer
fires the table's `Tick` once a second, and in-process channels carry `BallMoved` to the players
and their paddle moves back to the table. The terminal is redrawn from the table's `MatchState`
view after every tick, with the table's non-zero counters under the grid. It stops when a side has
won, at three points by default:

```console
$ b10x-spindle play examples/rally/ess --environment examples/environments/laptop.toml
```

The first frame of the run:

```text
west needs 3    east needs 3
+----------------+
|                |
|                |
|                |
|                |
|]       o      [|
|                |
|                |
|                |
|                |
+----------------+
command.rally.table.OpenMatch.opened 1
event.rally.table.BallMoved.appended 1
binding.east-watches.sent 1
binding.west-watches.sent 1
store.rows_written 1
store.bytes_written 359
```

and the last of its 31 frames, 30 seconds later, with exit status 0:

```text
west needs 3    east needs 0
+----------------+
|                |
|                |
|]              [|
| o              |
|                |
|                |
|                |
|                |
|                |
+----------------+
East wins
command.rally.table.MovePaddle.west-up 12
command.rally.table.MovePaddle.west-down 10
command.rally.table.MovePaddle.east-up 12
command.rally.table.MovePaddle.east-down 10
command.rally.table.OpenMatch.opened 1
command.rally.table.Tick.served 3
command.rally.table.Tick.east-wins 1
command.rally.table.Tick.east-scores 2
command.rally.table.Tick.top-wall 2
command.rally.table.Tick.bottom-wall 1
command.rally.table.Tick.moved-west-up 11
command.rally.table.Tick.moved-west-down 10
event.rally.table.BallMoved.appended 28
event.rally.table.MatchWon.appended 1
event.rally.table.PaddleMoved.appended 44
event.rally.table.PointScored.appended 3
binding.east-moves.delivered 22
binding.east-watches.sent 28
binding.west-moves.delivered 22
binding.west-watches.sent 28
timer.tick.fired 30
store.rows_written 75
store.bytes_written 23947
```

`play` refuses an environment this one-process host cannot run. With `microvm.toml` the channel
is bound to `tcp`, and the host carries bindings in process only:

```console
$ b10x-spindle play examples/rally/ess --environment examples/environments/microvm.toml
b10x-spindle: unsupported: rally-player-east has its channel bound to tcp; this host carries bindings in-process only
$ echo $?
1
```

## See what it counts

The counters under the grid are derived from the specification, like the capabilities. `metrics`
prints every counter and histogram one component reports, and the part of the runtime that
measures it ([observe](/docs/concepts/observe)):

```console
$ b10x-spindle metrics examples/rally/ess --component rally-player-west
command.rally.west.ObserveBall.first-sight counter kernel
command.rally.west.ObserveBall.up counter kernel
command.rally.west.ObserveBall.down counter kernel
command.rally.west.ObserveBall.hold counter kernel
command.rally.west.ObserveBall.failed.unsupported counter kernel
command.rally.west.ObserveBall.failed.invalid_input counter kernel
command.rally.west.ObserveBall.failed.undecidable counter kernel
command.rally.west.ObserveBall.failed.undecided counter kernel
command.rally.west.ObserveBall.failed.no_value counter kernel
command.rally.west.ObserveBall.failed.invalid_value counter kernel
command.rally.west.ObserveBall.failed.broken_invariant counter kernel
command.rally.west.ObserveBall.failed.undeclared counter kernel
command.rally.west.ObserveBall.failed.store counter kernel
command.rally.west.ObserveBall.failed.not_granted counter kernel
command.rally.west.ObserveBall.duration_us histogram kernel
event.rally.west.BallSeen.appended counter store
event.rally.west.PaddleMoveRequested.appended counter store
binding.west-moves.sent counter channel
binding.west-moves.redelivered counter channel
binding.west-moves.dropped counter channel
binding.west-watches.delivered counter channel
binding.west-watches.dropped counter channel
store.rows_written counter store
store.bytes_written counter store
```

## Next

- [The rally example](/docs/examples/rally) explains the three components and their bindings.
- [CLI reference](/docs/reference/cli) has `run`, which executes commands read as JSON lines or replays
  an `ess-scenario` document with `--scenario`.
- [Status](/docs/status) says what is shipped and what is planned.

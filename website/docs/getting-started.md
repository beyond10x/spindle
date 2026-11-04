---
title: Getting started
sidebar_position: 2
description: Build b10x-spindle from source, then compile, derive, admit and play the rally example.
lede: Build the binary, then take the rally example through check, capabilities, admit and play.
source: every command below was run in a checkout of main on 2026-11-04; output pasted from that run
---

Spindle has no release yet. Build it from source.

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
   Compiling spindle-cli v0.1.0 (…/spindle/crates/spindle-cli)
    Finished `release` profile [optimized] target(s) in 1m 05s
```

The binary is `target/release/b10x-spindle`. The commands below call it `b10x-spindle`; put
`target/release` on your `PATH` or type the path.

```console
$ b10x-spindle --version
b10x-spindle 0.1.0
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

Nobody wrote this list down: it comes from the entities, bindings and components in the
specification ([capabilities](/docs/concepts/capabilities)). `--component` narrows it to what one
component needs on its own:

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

Admission can refuse. Spindle's own specification (`ess/`) has two outcomes decided outside the
model, and the `microvm` environment provides nothing for them:

```console
$ b10x-spindle admit ess --environment examples/environments/microvm.toml
refused on microvm: no implementation for
  external.spindle.deployment.DecideAdmission.refused
  external.spindle.placement.PlaceInstance.refused
$ echo $?
3
```

Exit status 3 means refused ([environment and admission](/docs/concepts/environment-and-admission)).

## Play it

`play` runs all three rally components in one process: each is admitted on its own, the timer
fires the table's `Tick` once a second, and in-process channels carry `BallMoved` to the players
and their paddle moves back to the table. The terminal is redrawn from the table's `MatchState`
view after every tick until a side has won (three points by default):

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
```

and the last, 30 seconds and 31 frames later, with exit status 0:

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
```

`play` refuses an environment this one-process host cannot run. With `microvm.toml` the channel
is bound to `tcp`, and the host carries bindings in process only:

```console
$ b10x-spindle play examples/rally/ess --environment examples/environments/microvm.toml
b10x-spindle: unsupported: rally-player-east has its channel bound to tcp; this host carries bindings in-process only
$ echo $?
1
```

## Next

- [The rally example](/docs/examples/rally) explains the three components and their bindings.
- [CLI reference](/docs/reference/cli) has `run`, which executes commands read as JSON lines.
- [Status](/docs/status) says what is shipped and what is planned.

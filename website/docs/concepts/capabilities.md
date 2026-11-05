---
title: Capabilities
sidebar_position: 2
description: The technical needs of a specification, derived from what it declares and never written down by its author.
lede: Capabilities are derived from the specification, never authored, so they cannot drift from it.
source: crates/spindle-capability/src/derive.rs, crates/spindle-capability/tests/admission.rs, crates/spindle-capability/tests/mint.rs, crates/spindle-cli/src/cli.rs, crates/spindle-cli/tests/decisions.rs, README.md
---

A capability is something technical a specification needs from the runtime: a durable store, a
timer, a channel between components. Spindle reads the compiled specification and lists them; the
author never writes a requirement list, so the list cannot claim less than the specification
needs.

## The derivation rules

| Capability | Derived from |
|---|---|
| `store.durable` | an entity in a domain the deployment owns |
| `timer`, `clock` | a periodic binding |
| `channel` | a binding from one component's event to another component's command |
| `ingress.network` | a component with `reached_by: network` |
| `ingress.command_line` | a component with `reached_by: command_line` |
| `projection` | a view with `consistency: eventual` |
| `external.<command>.<outcome>` | an outcome whose condition is `external:` |
| `identity.mint` | a `creates:` outcome whose identity is neither an input nor a payload source |

Each capability carries its reasons: the kind of declaration (`entity`, `binding`, `component`,
`view`, `outcome`), its name, and what about it needs the capability. `b10x-spindle capabilities`
prints them, `--json` as JSON.

## Per component

Without `--component`, the whole system is one deployment, and only a binding between two
components needs a `channel`. With `--component <name>`, the list is what that component needs
when it runs on its own: the entities of the domains it owns, the commands it accepts, and the
bindings that start or end at it.

```console
$ b10x-spindle capabilities examples/rally/ess --component rally-table
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
  entity rally.table.Match: its instances outlive the command that changed them
timer
  binding tick: a periodic cause invokes rally.table.Tick
```

## Adding one

A new capability is a new derivation rule in `crates/spindle-capability/src/derive.rs` with a test.
A specification never gains a field that lists capabilities.

## Answering external outcomes

An `external.*` capability is an outcome the model cannot decide on its own. `b10x-spindle run`
answers it per command: from the line's `"external"` object, then from a `--decisions` TOML file
(`<outcome> = true | false` at the top level for every command, or under a `["<command>"]` table
for one), then, with `--answer-default`, "does not hold" on a command that declares a default
branch. A line nothing answers fails, naming the outcome, and the run goes on.

## Serving `ingress.network` over HTTP

Where a component requires `ingress.network` and the environment binds it to `http-loopback`,
`b10x-spindle run` runs every component in one process and serves those components over HTTP on
`127.0.0.1` instead of reading stdin. Any other binding, such as `http` in `microvm.toml`, is
refused before anything listens. Once listening it prints
`{"listening": "127.0.0.1:<port>", "components": [...]}`; `--port` picks the port, `0` by default
for one the system chooses. It serves until SIGINT or SIGTERM.

What is served is what ESS derives: each command the component accepts and, for a
`reached_by: network` component, each view of a domain it owns. Where it is served:

- **With an `ess-http/1` binding** (`--http <file>`, the document `ess specify http validate`
  checks), every route is the binding's, read and compiled by ESS: its method, path and path
  variables, and the status it lists per outcome. An operation the binding does not name keeps
  the route ESS derives for it, as `ess generate --kind openapi --http <file>` publishes it.
- **Without one, the fallback:** a command is `POST /api/commands/<qualified command name>` and a
  view `GET /api/views/<qualified view name>`, each outcome at the status ESS derives.

A command's request body is its input as ESS projects it, less any field the path carries. It
answers its outcome's status with

```json
{
  "outcome": "<declared outcome>",
  "error": {"error": "<qualified error name>", "payload": {}},
  "events": [{"event": "<qualified event name>", "payload": {}}],
  "invocations": [{"binding": "<id>", "component": "<name>", "command": "<qualified name>",
                   "outcome": "<declared outcome>", "events": []}]
}
```

`error` only on a refusing outcome, its `payload` only where the error declares fields;
`invocations` lists the commands the component's event bindings then ran (with `error` where the
model refused one, or `failure` where it did not run), empty when none fired. A view answers
`200` with the body ESS projects for it, `{"rows": [...]}`. A request the surface cannot read is
refused `400`, an unknown path `404` and a wrong method `405`, each `{"refused": "<why>"}`; a
command the runtime cannot complete answers `501` with `{"refused", "committed"}`.

---
title: Capabilities
sidebar_position: 2
description: The technical needs of a specification, derived from what it declares and never written down by its author.
lede: Capabilities are derived from the specification, never authored, so they cannot drift from it.
source: crates/spindle-capability/src/derive.rs, crates/spindle-capability/tests/admission.rs, README.md
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

:::caution[Planned]
Deriving `identity.mint` for identities the runtime must mint, serving `ingress.network` over HTTP,
and answering `external.*` outcomes from a decisions file are planned; see [status](/docs/status).
:::

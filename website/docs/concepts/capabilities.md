---
title: Capabilities
sidebar_position: 2
description: The technical needs of a specification, derived from what it declares and never written down by its author.
lede: Capabilities are derived from the specification, never authored, so they cannot drift from it.
source: crates/spindle-capability/src/derive.rs, crates/spindle-capability/tests/spindle-capability/admission.rs, crates/spindle-capability/tests/spindle-capability/mint.rs, crates/spindle-cli/src/cli.rs, crates/spindle-cli/tests/spindle-cli/decisions.rs, crates/spindle-cli/tests/spindle-cli/served_decisions.rs, README.md
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

A `run` serving `ingress.network` answers a request's external causes the same way, from the
decisions file and then `--answer-default`; a request carries no `"external"` object, and one
nothing answers is refused `501`, naming the outcome. The file answers per command and outcome,
never per input value, so every request to one command gets the same answer. Without either flag
a served specification that declares an external outcome is refused before anything listens.

## Serving `ingress.network` over HTTP

Where a component requires `ingress.network` and the environment binds it to `http-loopback`,
`b10x-spindle run` runs every component in one process and serves those components over HTTP on
`127.0.0.1` instead of reading stdin. Any other binding, such as `http` in `microvm.toml`, is
refused before anything listens. Once listening it prints
`{"listening": "127.0.0.1:<port>", "components": [...]}`; `--port` picks the port, `0` by default
for one the system chooses. It serves until SIGINT or SIGTERM.

What is served is what ESS derives: each command the component accepts and, for a
`reached_by: network` component, each view of a domain it owns. Each is served exactly where
`ess generate --kind openapi` publishes it:

- **Without a binding,** at the route ESS derives: `POST /<domain>/commands/<command>` and
  `GET /<domain>/views/<view>`, from their wire names, each outcome at the status ESS derives.
- **With an `ess-http/1` binding** (`--http <file>`, the document `ess specify http validate`
  checks), an operation it names is at its method, path and path variables, compiled by ESS, and
  answers the status it lists per outcome, as `ess generate --kind openapi --http <file>`
  publishes it. An operation it does not name keeps its derived route.

A command's request body is its input as ESS publishes it, less any field the path carries; a
field the command does not declare is refused. It answers its outcome's status with the envelope
ESS publishes for that outcome:

```json
{
  "outcome": "<declared outcome>",
  "published": [{"event": "<qualified event name>", "payload": {}}],
  "error": "<qualified error name>",
  "payload": {}
}
```

`error` only on a refusing outcome, `payload` only where the error carries fields. An outcome the
binding answers with `body: response` answers the command's response alone, with
`body: response.<field>` the value of that one field alone, both as `application/json`, and with
`body: none` no body and no `Content-Type`. A refusing outcome the binding answers with an error
object, `body: {<member>: error}`, answers its bound status with an object whose `<member>` is
the error's wire name, beside the error's fields under their wire names, and writes nothing. An
outcome the binding answers with headers, a redirect or the `text` encoding answers `501` naming
the outcome and which of those it is, and writes nothing. A view answers
`200` with the body ESS publishes for it, `{"rows": [...]}`. Refusals the surface makes, as ESS's
served application makes them, are `{"refused": "<why>"}`: `400` for a request it cannot read
(an undeclared field, a broken path escape, a body that breaks off), `404` for an unknown path,
`405` for a method other than the route's (compared exactly), `411` for a chunked body, `413` for
one announced over 1 MiB, `408` for a body that does not arrive in time. A command the runtime
cannot complete answers `501` with `{"refused", "committed"}`.

---
title: Components and bindings
sidebar_position: 4
description: How the components of one ESS system run together, and how periodic and event bindings drive their commands.
lede: Components own domains and accept commands; bindings are the only way one drives another.
source: crates/spindle-host/src/lib.rs, crates/spindle-timer/src/lib.rs, crates/spindle-timer/tests/spindle-timer/periodic.rs, crates/spindle-channel/src/lib.rs, crates/spindle-channel/tests/spindle-channel/channel.rs, examples/rally/ess/components.yaml
---

## Components

An ESS system declares **components**. Each owns one or more domains, accepts commands, publishes
events and says how it is reached (`reached_by: network`, `command_line` or `in_process`). A
component is the unit Spindle admits and places: `--component` derives and admits for one of them.
The one-process host admits each component on its own and gives each its own store.

## Bindings

A **binding** invokes a command. Its cause is either periodic or an event, and it maps fields into
the command's input. Spindle runs both kinds from the compiled IR. They are not configured outside
the specification.

### Periodic bindings

`spindle-timer` reads every periodic binding and answers, for an instant from a clock, which are
due. The clock is a port, so tests run a system on a manual clock without sleeping. ESS 0.85.0
admits one periodic profile, and the timer honours each word of it:

| Word | Value | Meaning |
|---|---|---|
| `anchor` | `host_activation` | the grid starts when the binding's lifetime is activated |
| `first` | `after_period` | nothing is due at activation; the first tick is one period later |
| `cadence` | `fixed_rate` | tick *n* is due at activation + *n* × period |
| `overlap` | `serial_per_instance` | no second tick while one is still open |
| `missed` | `one_pending_drop_excess` | missed deadlines keep one pending tick; the rest are dropped and counted |

### Event bindings

`spindle-channel` carries an event published by one component to the command it binds to in
another:

- A sender maps each event into one envelope per binding and holds it in an **outbox** until the
  state that produced the event is durable. Only then is it sent.
- `at_most_once` envelopes are sent once. `at_least_once` envelopes are acknowledged by the
  receiver. An envelope whose acknowledgement is overdue is not resent.
- Two channels implement the port: `in-process`, a pair of queues, and `tcp`, length-prefixed JSON
  frames over a TCP stream.

A binding inside one component needs no channel: the runtime dispatches it in process.

## One process

`spindle-host` runs every component of a system in one process: per-component admission and store,
the timer on an injected clock, and in-process channels between every pair of components a binding
joins. A command's events are carried to the commands they bind to, and those run on their own
components, until nothing is left in flight. A view is answered from the store of the component
that owns its source. `b10x-spindle play` is this host driving the
[rally example](/docs/examples/rally).

:::caution[Planned]
Components in separate processes or microVMs joined by the `tcp` channel are planned
([status](/docs/status)).
:::

---
title: Spindle
sidebar_label: Overview
kicker: Start
sidebar_position: 1
description: Spindle runs ESS specifications. It derives the capabilities a specification needs, admits it against an environment, and executes the compiled model.
lede: A specification says what an application means; Spindle supplies everything technical underneath it.
source: README.md, docs/design/spindle.md, crates/spindle-spec, crates/spindle-capability, crates/spindle-kernel, crates/spindle-host
---

Spindle runs [ESS](https://beyond10x.github.io/ess/) ([GitHub](https://github.com/beyond10x/ess))
specifications. A specification says what an application means: entities, commands, guards,
events, bindings, views. Spindle supplies everything technical underneath: where state is stored,
how a timer fires, how an event of one component reaches a command of another.

It works in four steps, one `b10x-spindle` command each:

| Step | Command | What happens |
|---|---|---|
| Compile | `check` | `ess-compiler` turns the specification directory into the resolved IR. |
| Derive | `capabilities` | Spindle lists the **capabilities** the IR needs, each with the declaration that needs it. |
| Admit | `admit` | An **environment** names implementations; admission binds one to every capability, or refuses and names every one missing. |
| Execute | `run`, `play` | Commands run against the compiled model; periodic bindings fire on a timer; channels carry events between components. |

No code is generated per application. The compiled model is the program, and guards are ESS's own
predicates evaluated by ESS. The same specification runs on a laptop with an in-process channel and
on a TCP channel elsewhere. Admission is the only step that differs.

:::caution[Early]
Loading, capability derivation, admission, command execution with list views and minted
identities, external decisions from a file, per-component counters and HTTP ingress work today.
The rally example runs in one process, as three processes and as three microVMs.
`b10x-spindle deploy` launches the instances of a placement as processes and serves the live
observe API and UI. The process, QEMU and Firecracker launchers exist as libraries and sample each
instance's CPU and memory.

A bundle uploaded to deploy, and cells with ownership leases, are planned. The
[status page](/docs/status) lists each item with the test that checks it.
:::

## What Spindle is not

- Not a code generator: it emits no Rust, TypeScript or SQL for an application. It executes the
  compiled IR.
- Not a second ESS: it does not parse, validate or evaluate ESS on its own. A document
  `ess specify validate` refuses is refused by Spindle with the compiler's diagnostics.
- Not a place for technical words in a specification: a specification does not name a database, a
  protocol or a queue. Those live in an environment document.

## Neighbours

- [ESS](https://beyond10x.github.io/ess/) ([GitHub](https://github.com/beyond10x/ess)) specifies
  what Spindle runs. Spindle depends on `ess-domain`, `ess-compiler` and `ess-primitives` at a
  released tag, and its own behaviour is specified in ESS (`ess/` in the repository).

## Where to go next

- [Getting started](/docs/getting-started): build `b10x-spindle` and run it on the rally example.
- [Concepts](/docs/concepts/specification): specification, capabilities, environment and
  admission, components and bindings, deployment targets, observe.
- [The rally example](/docs/examples/rally): three components, one tick per second, a winner.
- [CLI reference](/docs/reference/cli): every command and argument.

---
title: Environment and admission
sidebar_position: 3
description: An environment names the implementations available where Spindle runs; admission binds one to every capability or refuses.
lede: The environment, not the specification, chooses the implementations, and admission is the only step that differs between a laptop and a VM.
source: crates/spindle-capability/src/environment.rs, crates/spindle-capability/src/admission.rs, crates/spindle-capability/tests/spindle-capability/admission.rs, crates/spindle-cli/tests/spindle-cli/admit.rs, crates/spindle-host/src/host.rs
---

## The environment

An environment document (`spindle-environment/1`, TOML) says where Spindle runs and which
implementations provide which capabilities there:

```toml
format = "spindle-environment/1"
name = "microvm"
os = "linux"
arch = "x86_64"

[provides]
"store.durable" = ["sqlite"]
"store.ephemeral" = ["memory"]
"identity.mint" = ["uuid-v4"]
clock = ["system"]
timer = ["tokio"]
channel = ["tcp"]
projection = ["in-process"]
"ingress.network" = ["http"]
```

- Each list is in order of preference.
- A key ending in `.*` provides every capability under that prefix; an exact key wins over it.
- A document in another format, an unknown field, or a capability with an empty list is refused.

The repository ships two: `examples/environments/laptop.toml` and
`examples/environments/microvm.toml`.

## Admission

Admission takes the capabilities a specification (or one component) requires and the environment,
and answers one of two things:

- **admitted**, with exactly one implementation bound to every required capability: the first one
  the environment lists for it;
- **refused**, naming every capability the environment has no implementation for.

The specification never chooses an implementation. `b10x-spindle admit` prints the result and
exits 0 when admitted, 3 when refused; `--json` prints it as JSON.

```console
$ b10x-spindle admit examples/rally/ess --environment examples/environments/microvm.toml
admitted on microvm
  channel = tcp
  clock = system
  ingress.network = http
  store.durable = sqlite
  timer = tokio
```

## What the runtime does with a binding

Admission binds names. The parts that run today act on these bindings:

| Capability | Implementations the runtime runs today |
|---|---|
| `store.durable` | `memory`, `sqlite` (one SQLite file per component; `run --state` names it), `indexeddb` (one IndexedDB database per component, in the Service Worker host) |
| `channel` | `in-process` in the one-process host (`play`); the `tcp` channel exists as a library |
| `timer` | `tokio`, the name the shipped environments give `spindle-timer`'s schedule |
| `clock` | `system` |
| `identity.mint` | `uuid-v4` |
| `ingress.network` | `service-worker`: the Service Worker host (`spindle-wasm`, the site in `web/`) answers the specification's routes and the console routes in the browser, through the router the loopback listener calls |
| `external.*` | `decisions-file` in the Service Worker host: each external outcome answered from the decisions file it is opened with, as `run --decisions` answers it |

The one-process host refuses, as unsupported, a binding it cannot run, such as a `tcp` channel or
any `external.*` outcome. `ingress.network` is admitted but not served yet.

:::caution[Planned]
HTTP ingress for `ingress.network` and the `tcp` channel between separate processes are planned;
see [status](/docs/status).
:::

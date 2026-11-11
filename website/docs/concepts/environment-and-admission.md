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
- A key ending in `.*` provides every capability under that prefix, and an exact key wins over it.
- A document in another format, an unknown field, or a capability with an empty list is refused.

The repository ships four: `examples/environments/browser.toml`,
`examples/environments/laptop.toml`, `examples/environments/laptop-processes.toml` and
`examples/environments/microvm.toml`.

## Admission

Admission takes the capabilities a specification (or one component) requires and the environment,
and answers one of two things:

- **admitted**, with exactly one implementation bound to every required capability: the first one
  the environment lists for it
- **refused**, with every capability the environment has no implementation for

The specification does not choose an implementation. `b10x-spindle admit` prints the result and
exits 0 when admitted, 3 when refused, and `--json` prints it as JSON.

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

Admission binds names, and the parts that run today act on these bindings:

| Capability | Implementations the runtime runs today |
|---|---|
| `store.durable` | `memory`, `sqlite` (one SQLite file per component; `run --state` names it; the store keeps each entity's rows as they rest, read from the file once when it opens and then from each write, so a view read costs the rows it reads, not the file's history), `indexeddb` (one IndexedDB database per component, in the Service Worker host) |
| `channel` | `in-process` in the one-process host (`play`, `run`); `tcp` between processes, one component each (`play --component`) |
| `timer` | `tokio`, the name the shipped environments give `spindle-timer`'s schedule |
| `clock` | `system` |
| `identity.mint` | `uuid-v4` |
| `ingress.network` | `http-loopback`: `run` serves the routes and the console routes over HTTP on 127.0.0.1; `service-worker`: the Service Worker host (`spindle-wasm`, the site in `web/`) answers the specification's routes and the console routes in the browser, through the router the loopback listener calls |
| `external.*` | answered by `run` from the line, `--decisions` or `--answer-default`, whatever the binding names, unless it names an HTTP call the environment declares (below); `decisions-file` in the Service Worker host: each external outcome answered from the decisions file it is opened with, as `run --decisions` answers it |

A host refuses, as unsupported, a binding it cannot run: the one-process host refuses a `tcp`
channel. A host opened with nothing to answer external causes refuses every `external.*` outcome.
`run` serves `ingress.network` only where it is bound to `http-loopback`, and the Service Worker
host only where it is bound to `service-worker`. Any other binding of it is refused before anything
is served.

## An external cause bound to an HTTP call

An environment may declare HTTP calls, one `[http.<name>]` table each. It binds an external cause
to a call by listing the call as the implementation of the cause's `external.<command>.<outcome>`
key:

```toml
[provides]
"external.shop.order.Place.approved" = ["approvals"]
"external.*" = ["operator-prompt"]

[http.approvals]
method = "POST"
url = "http://127.0.0.1:8080/approvals"
timeout_ms = 2000
secret_env = "APPROVALS_TOKEN"
```

Each time `run` asks that cause, on a stdin line or a served request, it sends one request to the
call. The body is `spindle-external/1` JSON with `format`, `capability`, `command`, `outcome`,
`cause` and the command's `input`.

| Reply | Result |
|---|---|
| `2xx` with `{"holds": true}` or `{"holds": false}` | answers the cause, and the outcome it selects is committed |
| a timeout, a failure, another status, a body over 64 KiB or another body | the command fails as `undecided` and writes nothing; a served request is answered `501` |

A call holds the thread that runs every command until it is answered or times out. A served run
runs no other command meanwhile: commands, and the reset, run one at a time in arrival order.
Reads do not wait for it. A view read, `GET /api/model`, `GET /api/events` and every
`GET /__spindle/` route are answered beside that thread, from the state the last finished command
left.

The document never holds the secret. `secret_env` is the name of an environment variable, read
when `run` starts and sent as `Authorization: Bearer <value>`. An unset variable stops `run` before
the first request.

These are refused when the document is read. The message gives the table's name, never the
value:

- a table that no `external.` key uses
- a `timeout_ms` outside 1 to 30000
- a `url` carrying userinfo (`user:password@`)
- a `secret_env` that is not a variable name

This build calls `http://` URLs only. A served run whose every external cause is bound to a call
needs neither `--decisions` nor `--answer-default`. With `--observe`, a cause a call answers is
listed and counted with the source `call`, and one whose call failed as `unanswered`.

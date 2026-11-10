---
title: Observe
sidebar_position: 6
description: What a running Spindle instance counts without the specification saying anything about metrics, the observe API contract and the observe UI.
lede: Every effect of an instance passes through the runtime, so the runtime counts it, and the counters are derived from the specification like capabilities.
source: crates/spindle-telemetry/src/lib.rs, crates/spindle-target/src/process.rs, crates/spindle-qemu/src/lib.rs, crates/spindle-firecracker/src/lib.rs, crates/spindle-telemetry/tests, crates/spindle-target/src/sample.rs, crates/spindle-target/tests/spindle-target/sampling.rs, ess/domains/observe.yaml, Taskfile.yml, ui/, docs/design/observe.md
---

## Counters

`spindle-telemetry` counts what an instance does:

| Recorder | Records |
|---|---|
| the kernel's caller | each command's outcome and duration |
| a counting store | each committed row, its bytes and its events |
| the channel endpoints | each envelope sent, delivered, redelivered or dropped |
| the timer | each tick fired, late or dropped |

Recording is one atomic increment: no lock and no allocation on the command path.

The **catalogue** of counters is derived from the compiled specification for one component. It is
not written down:

| Counter | One per |
|---|---|
| `command.<Command>.<outcome>` | outcome of each command the component accepts |
| `command.<Command>.failed.<kind>` | way the kernel can fail to execute it |
| `command.<Command>.duration_us` (histogram) | command it accepts |
| `event.<Event>.appended` | event it publishes or its commands emit |
| `binding.<binding>.<sent, redelivered, dropped>` | event binding whose event it publishes |
| `binding.<binding>.<delivered, dropped>` | event binding whose command it accepts |
| `timer.<binding>.<fired, late, dropped>` | periodic binding whose command it accepts |
| `store.rows_written`, `store.bytes_written` | component owning an entity |

Names use the ESS handles as written, so a name in the catalogue is the name in the specification.
`b10x-spindle metrics <spec> --component <name>` prints the catalogue, and `b10x-spindle play`
prints the table's non-zero counters under the grid
([getting started](/docs/getting-started#see-what-it-counts)).

## Resources

Each launcher samples what an instance costs the host from `/proc`: CPU seconds (user and system
time) and resident memory, and its share of the vCPUs and memory the instance asked for. A process
instance without a limit reports no quota.

## Telemetry

An instance that starts the `spindle-telemetry` sender sends one `spindle-telemetry/1` frame every
sample period (1 s by default) to the endpoint its launcher gives it in `SPINDLE_TELEMETRY`:

| Target | Endpoint |
|---|---|
| process | `unix:<path>`, a Unix socket the caller listens on (`ProcessLauncher::with_telemetry`) |
| QEMU microVM | `vsock:2:<port>`, a host vsock port; each guest gets a `vhost-vsock-device` (`QemuLauncher::with_telemetry`) |
| Firecracker | `vsock:2:<port>`, forwarded by Firecracker to the Unix socket `vsock.sock_<port>` in the instance directory (`Firecracker::with_telemetry`) |

A frame is one line of JSON with the shape of `spindle.observe.Sample`: the instance id, a
sequence number, the cumulative counters and histogram buckets, the last answers and the
instance's own readings. The readings are CPU seconds and memory in use, from `/proc/self` for a
process and from `/proc/stat` and `/proc/meminfo` inside a guest. Counters are cumulative, so a
lost frame loses no count.

The sender runs on its own thread. With no collector, or a slow one, it drops frames and reports
the number dropped in the next frame it delivers. A command does not wait for it. A frame counts as
sent once it is written to the socket, which does not prove a collector read it.

A Unix socket path must be UTF-8 and fit the platform's `sun_path` less its NUL: at most 107 bytes on Linux and
103 on macOS. The launcher refuses any other.

## The observe API and UI

Spindle's own specification has a domain `spindle.observe` for the answers of the observe API:
the deployment, the counter catalogue, the sample and the state change. `ess` generates their JSON
Schema, Rust types (`crates/spindle-observe-api`) and TypeScript types (`ui/src/api/generated`).
`task check` fails when a committed projection differs from a fresh generation or a fixture answer
does not validate.

`ui/` is a Vue 3 app with a topology, an instance and a view screen. It is built and tested on
those fixture answers: `pnpm dev` in `ui/` serves it with the fixtures on `127.0.0.1:5173` and
prints the URL to open, with the development token `dev`.

## The control process

`b10x-spindle deploy` places and launches the instances of a `spindle-placement/1` document, each
on its target as `spindle.placement.PlaceInstance` decides. It collects the frames they send over a
Unix socket and keeps the last 15 minutes of samples per instance in memory. It serves the observe
API and the UI on `127.0.0.1`. The UI is built into the binary at compile time from `ui/dist`, so
`deploy` needs nothing installed. `task build` builds `ui/` before cargo.

```console
task build
target/release/b10x-spindle deploy examples/rally/ess --environment examples/environments/laptop-processes.toml --placement examples/placements/rally-processes.toml
```

It prints where it listens and the URL to open, with a random token. Every request needs that
token, as `?t=` or as a bearer credential, and a `Host` header of `127.0.0.1:<port>` or
`localhost:<port>`. Only `GET` is answered, on these routes:

| Route | Answers |
|---|---|
| `GET /api/deployment` | the components, bindings and instances with their state |
| `GET /api/stream` | one `sample` per instance per period and one `state` per state change |
| `GET /api/views/<component>/<view>` | the view's answer from the ingress of a running instance of a component reached over the network |

SIGINT or SIGTERM stops every instance.

:::caution[Planned]
This build launches instances on `Process` targets only. QEMU and Firecracker targets are refused,
and the refusal message gives the target kind. See [status](/docs/status) and
[`docs/design/observe.md`](https://github.com/beyond10x/spindle/blob/main/docs/design/observe.md).
:::

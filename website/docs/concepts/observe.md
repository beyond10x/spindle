---
title: Observe
sidebar_position: 6
description: What a running Spindle instance counts without the specification saying anything about metrics, the observe API contract and the observe UI.
lede: Every effect of an instance passes through the runtime, so the runtime counts it, and the counters are derived from the specification like capabilities.
source: crates/spindle-telemetry/src/lib.rs, crates/spindle-telemetry/tests, crates/spindle-target/src/sample.rs, crates/spindle-target/tests/sampling.rs, ess/domains/observe.yaml, Taskfile.yml, ui/, docs/design/observe.md
---

## Counters

`spindle-telemetry` counts what an instance does: the kernel's caller records each command's
outcome and duration, a counting store each committed row, its bytes and its events, the channel
endpoints each envelope sent, delivered, redelivered or dropped, and the timer each tick fired,
late or dropped. Recording is one atomic increment: no lock and no allocation on the command path.

The **catalogue** of counters is derived from the compiled specification for one component, never
written down:

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

## The observe API and UI

Spindle's own specification has a domain `spindle.observe` for the answers of the observe API:
the deployment, the counter catalogue, the sample and the state change. `ess` generates their JSON
Schema, Rust types (`crates/spindle-observe-api`) and TypeScript types (`ui/src/api/generated`), and
`task check` fails when a committed projection differs from a fresh generation or a fixture answer
does not validate.

`ui/` is a Vue 3 app with a topology, an instance and a view screen. It is built and tested on
those fixture answers: `pnpm dev` in `ui/` serves it with the fixtures on `127.0.0.1:5173` and
prints the URL to open, with the development token `dev`.

:::caution[Planned]
Nothing serves the observe API yet. Sending samples from each instance (`spindle-telemetry/1`
frames over a Unix socket or vsock) and `b10x-spindle deploy`, a control process that launches the
instances, collects their samples, serves the API on `127.0.0.1` and embeds the UI, are planned.
Until then the UI shows fixtures, not a running system. See [status](/docs/status) and
[`docs/design/observe.md`](https://github.com/beyond10x/spindle/blob/main/docs/design/observe.md).
:::

---
title: Observe
sidebar_position: 6
description: What a running Spindle instance counts without the specification saying anything about metrics, the observe API contract and the observe UI.
lede: Every effect of an instance passes through the runtime, so the runtime counts it, and the counters are derived from the specification like capabilities.
source: crates/spindle-telemetry/src/lib.rs, crates/spindle-telemetry/src/frame.rs, crates/spindle-control/src/collect.rs, crates/spindle-ingress/src/observe.rs, crates/spindle-ingress/src/observe/metrics.rs, crates/spindle-target/src/process.rs, crates/spindle-qemu/src/lib.rs, crates/spindle-firecracker/src/lib.rs, crates/spindle-telemetry/tests, crates/spindle-target/src/sample.rs, crates/spindle-target/tests/spindle-target/sampling.rs, ess/domains/observe.yaml, Taskfile.yml, ui/, docs/design/observe.md
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

The **catalogue** of counters for one component is derived from the compiled specification:

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

## A served run's metrics and request log

`b10x-spindle run --observe` answers `GET /__spindle/metrics` in the Prometheus text format and
`GET /__spindle/requests` with the last 256 request records, newest first.
[`--observe-requests`](../reference/cli.md#run--observe-requests) sets how many request records it
keeps, from 1 to 10000.

A scrape writes a series only once something was counted in it, and keeps writing it after that.
So a model of hundreds of commands that has run none writes two series, `spindle_resets_total`
and `spindle_metrics_series_mode`:

| Family | Written from |
|---|---|
| `spindle_commands_total`, `spindle_command_failures_total`, `spindle_command_duration_seconds` | the first command of that name counted with that outcome or failure |
| `spindle_events_appended_total` | the first event of that type appended |
| `spindle_bindings_total`, `spindle_binding_retries_total`, `spindle_timer_ticks_total` | the first envelope, retry or tick of that binding |
| `spindle_store_rows_written_total`, `spindle_store_bytes_written_total` | the component's first write |
| `spindle_entity_rows` | the first scrape that finds the entity holding a row; it stays, at 0, after a reset |
| `spindle_http_requests_total`, `spindle_http_request_duration_seconds`, `spindle_external_causes_total` | the first request or cause counted with those labels |
| `spindle_bound_refusals_total` | the first refused bound command a request record listed with that binding, command and refusal |
| `spindle_metrics_scrape_duration_seconds` | the second scrape; it reports how long the scrape before it took |
| `spindle_resets_total`, `spindle_metrics_series_mode` | the first scrape |
| `spindle_seed_commands_total` | the first scrape of a run given a [seed](environment-and-admission.md#a-seed-submitted-at-start), each of `sent`, `applied`, `already_there` and `refused`; a run without one never writes it |

`spindle_http_request_duration_seconds` is labelled by `method` and `route`, the route template,
so a slow route shows in its own series. An unrouted request has an empty `route`.

[`--observe-series`](../reference/cli.md#run--observe-series) sets how a scrape writes the labels
whose values come from the specification: `route`, `operation`, `command`, `outcome`, `event`,
`binding` and `entity`, and the `refusal` of a bound command where it names an outcome the
specification declares. The scrape names the mode in `spindle_metrics_series_mode{mode}`, at 1.

| Mode | Series a scrape writes |
|---|---|
| `per-command`, the default | one for each value counted, so the series grow with the specification: a generated model whose commands were each answered once writes about 33 sample lines per command |
| `aggregated` | each of those labels empty, and the series that then share every label summed. The number of series does not grow with the number of commands or outcomes, nor with the events, bindings or entities the specification declares. It is bounded by the labels kept: a request's method, kind, class and status class, a failure kind, what a binding carried, retried or timed, a cause's source, a seed entry's count, the kernel's error kind of a rejected bound command, and `component`, which keeps one value per declared component. Generated models of 30 and of 300 commands, each answered once, write the same series |

The mode changes the scrape only. The request log and the topology keep every route and operation,
in either mode.

| Request log field | Holds | At most |
|---|---|---|
| `path` | the route template the request resolved to, `/loans/{id}` for a request to `/loans/abc-123`. Of a request no route serves, the first segment, `/loans/…` | 2,048 bytes |
| `reason` | the refusal text, with every value and every map key, struct member or input field name the caller sent written `…`. A refused field is named with the type it expected | 512 bytes |
| `query` | the query string of a console request (`/api/model`, `/api/events`, `/api/history`, `/api/reset`): `after` with the cursor only where the read was answered, `limit` with its value only where it is digits, every other name and value written `…` | 2,048 bytes |

A longer field is cut with `…`. So the log holds no identifier a caller put in a path, and the
record of a refused request holds its method, its path and the names of its route, and under
1 KiB besides, whatever its body held. A poller's cursor is in the log, and no other value it
sent.

The `reason` of each binding listed under `unmet` is bounded the same way: it gives the binding,
the input and the type it expected, and not the value the event carried.

Under `refused_bound`, a record lists each command an event binding invoked on the way that was
refused, so nothing it would have written was written. An entry gives the binding, the component,
the command, `caused_by` (the command whose event the binding carried), how many attempts ran, and
why: the refusing `outcome` and the `error` it declares, or, where the kernel rejected the command,
its error `kind` (`broken_invariant`, `invalid_input` and the rest). An entry holds no value of the
error's fields. `GET /api/events` lists only the events the run committed, so a refused command
shows there by its absence alone. `spindle_bound_refusals_total{binding, command, refusal}`
counts the entries, `refusal` being the outcome or the kind. A command a timer fired, and one
refused in another process of a run split over processes, is in no record.

## A served run's seed, readiness and liveness

Every served `run` answers three routes of its own, with or without `--observe`, so a container
healthcheck or an orchestrator's probes need no view row:

| Route | Answers |
|---|---|
| `GET /__spindle/seed` | the state of the run's [seed](environment-and-admission.md#a-seed-submitted-at-start), `{"state": "none"}` on a run without one; otherwise `{"state", "entries", "left", "sent", "applied", "already_there", "refused", "rejected"}`, `state` being `running` or `ended` |
| `GET /__spindle/ready` | `200` where the run has no seed or its seed has ended, and no signal has arrived; `503` otherwise. The body is `{"ready", "seed", "stopping"}`, `seed` being the `state` above |
| `GET /__spindle/live` | `200` while the run's thread answers a request it took, however long that takes (a reset seeding again, a delivery retried), or has asked for one within the last 60 s; `503` once it holds none and has asked for none for 60 s. The body is `{"live", "idle_ms", "bound_ms"}`, `idle_ms` counting from its last ask |

```console
$ curl -s http://127.0.0.1:8080/__spindle/seed
{"already_there":0,"applied":1,"entries":3,"left":2,"refused":0,"rejected":0,"sent":1,"state":"running"}
```

The counts are those of the seed run under way, as the seed's end line prints them, not summed
over resets as `spindle_seed_commands_total` is. The seed reads `ended` only once the last entry's
command has returned, so a probe that reads `200` reads every view holding what that entry wrote. A
declared refusal among the entries leaves the run ready; a rejected one stops the run with exit 1.
On a `--state` whose `seed.json` records every entry done, the seed reads `ended` from the first
request. A `POST /api/reset` with no world reads `running` and `503` from before it empties the
stores until its seed run ends, with that run's counts.

All three are answered by the listener's own threads, never waiting for a command, a seed entry or
an external call. On a run with `--trusted-caller-header` they require the proxy's secret, as every
`/__spindle/` route does: a probe sends the `Spindle-Proxy-Secret` header, such as Kubernetes'
`httpGet.httpHeaders`. The answers are declared by `spindle.observe.SeedState`,
`spindle.observe.Readiness` and `spindle.observe.Liveness` in `ess/domains/observe.yaml`.

### Draining on SIGTERM

When SIGTERM or SIGINT arrives, `GET /__spindle/ready` answers `503` at once, even while a command
holds the run, and every request that arrives from then on is answered `503` at once; the three
routes above still answer. The command under way finishes. The requests already waiting are then
answered with their outcomes, in arrival order, for up to
[`--drain`](../reference/cli.md#run--drain) seconds, 20 by default, counted from when the command
under way finished; one taken within that bound finishes even past it. A `POST /api/reset` taken
in the drain is answered `503` and empties nothing. A bound too large for the clock sets no
deadline: the drain ends once no request is waiting. Each request still waiting
once the bound has passed is answered `503`, every answer is written, and the run exits 143 for
SIGTERM or 130 for SIGINT. `--drain 0` answers none of the waiting requests. So a rolling restart
fails no request already accepted, provided the orchestrator's grace period covers the external
call under way and the drain bound.

## Resources

A launcher samples from `/proc` what an instance costs the host: CPU seconds (user and system
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

A frame is at most 1 MiB of JSON (`FRAME_LIMIT`), and a frame carries at most 16 answers. One
frame holds a catalogue of 600 commands and 4,000 counters with names of up to 64 bytes. The sender
drops a longer frame and counts it. A collector ends a connection that sends a longer line.

A Unix socket path must be UTF-8 and fit the platform's `sun_path` less its NUL: at most 107 bytes
on Linux and 103 on macOS. The launcher refuses any other.

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
Unix socket and keeps the last 15 minutes of samples per instance in memory. It bounds what one
connection can make it hold:

- It closes a connection that sends no complete frame within five send periods, or at most
  5 minutes.
- It runs one reader per instance and eight more, and closes any connection over that at once.
- It drops and counts a frame when the queue to its supervisor is full.

It serves the observe API and the UI on `127.0.0.1`. The UI is built into the binary at compile
time from `ui/dist`, so `deploy` runs from the binary alone. `task build` builds `ui/` before cargo.

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

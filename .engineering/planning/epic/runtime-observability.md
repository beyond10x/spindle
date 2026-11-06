---
format: aep.planning-md/3
id: epic:runtime-observability
kind: epic
status: active
title: A served run shows what its ingress answered
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T14:45:46Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"review_outcome":1}}}
- {from: "proposed", to: "active", at: "2026-11-06T14:45:46Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"review_outcome":1}}}
---
# A served run shows what its ingress answered

## Outcome

`b10x-spindle run`, serving a specification over `http-loopback`, can be started with `--observe`.
With it, the same listener answers three routes under the reserved prefix `/__spindle/`:

- `GET /__spindle/metrics`: Prometheus text of what the run answered and executed: requests by
  route, method, class and status class, a latency histogram, commands by name and outcome, view
  reads, external causes by what answered them, rows per entity, resets, and every counter the
  telemetry catalogue already derives.
- `GET /__spindle/requests`: JSON, `spindle-requests/1`, the totals per class since start and the
  last N requests (route, bound command or view, class, outcome, the existing refusal text,
  duration, events emitted, external answers), newest first, filterable to `?class=not-served`.
- `GET /__spindle/topology`: JSON, `spindle-topology/1`, the graph of the served specifications
  (ingress, specifications, domains, commands, views, external causes, events, bindings), derived
  from the IR, with live numbers per edge from the same counters and ring.
- `GET /__spindle/ui`: one self-contained HTML page, embeddable in an iframe, that reads the
  routes above: a banner, the request list, and the live topology.

Without `--observe` nothing is recorded and every `/__spindle/` path answers `404`. A specification
route under `/__spindle/` is refused at start, flag or not.

## Scope

In:

- The nouns of the request log and the metric families in `spindle.observe`
  (`ess/domains/observe.yaml`), projected to JSON Schema, Rust (`spindle-observe-api`) and
  TypeScript (`ui/src/api/generated`), with fixtures validated against them.
- The `--observe` flag (and `--observe-requests <N>`, the ring's capacity) on `run` where it
  serves HTTP; the reserved prefix; the request recorder in `spindle-ingress`.
- The Prometheus exposition and the JSON request log on the run's own listener.
- The panel page, Vue 3 and TypeScript built by Vite into one inlined HTML file, embedded in the
  binary.
- Recording which source answered each external cause (reset world, decisions file,
  `--answer-default`, nothing).
- The live topology: the graph as JSON and as a screen of the panel; spanning several
  specifications once `story:one-ingress-several-specifications` lets one ingress serve them.

Out:

- `deploy` and the control process (`crates/spindle-control`): its observe API, token and UI stay
  as they are. Exposing the deployment's samples as Prometheus text (`docs/design/observe.md` § 7)
  is a later story.
- OTLP export, distributed tracing across components, per-envelope traces (§ 9 of the observe
  design).
- Authentication of the reserved routes: `run` listens on 127.0.0.1 unguarded today
  (`Listener::bind`), and the reserved routes follow it.
- Writes: nothing under `/__spindle/` changes the run; resetting stays `POST /api/reset`.
- Persistence of the request log across runs; a capacity above 10 000.
- Cross-origin reads (CORS) of the reserved routes: no `Access-Control-Allow-Origin` header
  (decision 1 below).

## What exists and what is new

Exists, reused:

| what | where | reused for |
|---|---|---|
| Per-component counters derived from the IR, atomic, no lock on the command path: commands by outcome and failure, duration histogram, events appended, bindings, timers, store rows and bytes | `crates/spindle-telemetry` (`catalogue`, `Counters`, `DURATION_BOUNDS_US`) | every command, event, binding, timer and store family of the exposition |
| The host holds one `Counters` per component, also when it serves HTTP | `crates/spindle-host/src/host.rs` (`Host::counters`, `shared_counters`) | the exposition reads them at scrape time |
| Fixed console routes beside the specification's, and the startup refusal of a specification route that a console route would shadow | `crates/spindle-ingress/src/console.rs`, `Surface::with_console`, `SurfaceError::Shadowed` (`surface.rs`) | the pattern for the reserved routes and for refusing a colliding route |
| The answer envelope with `refused` text, `501` unfinished, `403` not granted | `crates/spindle-ingress/src/answer.rs` | the request's class and its `reason`, verbatim |
| 404, 405, 400 answered on the listener thread without an `Exchange` | `crates/spindle-ingress/src/listener.rs` | where unrouted requests must be recorded |
| `spindle.observe`: the observe API nouns, generated Rust and TypeScript types, schema-validated fixtures, drift checks | `ess/domains/observe.yaml`, `Taskfile.yml` (`OBSERVE_ROOTS`, `check:*`) | the home and the pipeline for the new nouns |
| The deploy UI: Vue 3, TypeScript, Vite, embedded by a `build.rs` | `ui/`, `crates/spindle-control/build.rs`, `src/ui.rs` | toolchain, generated types and the embedding pattern; not its screens |
| Decision answering: reset world, then `--decisions`, then `--answer-default` | `crates/spindle-cli/src/decisions.rs` | wrapped to record what answered |

New:

- ESS types in `spindle.observe`: `RequestClass`, `OperationKind`, `AnswerSource`,
  `ExternalAnswer`, `RequestRecord`, `RequestTotals`, `RequestLog`, `MetricKind`, `MetricFamily`,
  `MetricFamilies`, `NodeKind`, `EdgeKind`, `TopologyNode`, `StatusCount`, `SourceCount`,
  `EdgeTraffic`, `TopologyEdge`, `Topology` (proposed file: `ess/domains/observe.yaml` in this
  directory).
- The topology derivation from the IR and the surface; a top-down SVG screen.
- A request recorder in `spindle-ingress`: the per-request record, the ring, the totals, the
  classification; the reserved-prefix refusal `SurfaceError::Reserved`.
- The Prometheus text writer; a rows-per-entity read on the host; a reset count.
- A second Vite build producing one inlined page; its embedding.
- An instrumented decider in the CLI.

## Decisions taken

- **Reserved prefix.** `/__spindle/` is reserved on every served run, with or without
  `--observe`, so turning the flag on never changes which specification routes exist. At start a
  route of the binding or of ESS's derived routes whose template has `__spindle` as its first
  literal segment is refused (`SurfaceError::Reserved`, exit 1 before listening, naming the
  operation, method, path and prefix). On every request, flag on or off, the reserved prefix is
  matched before any specification route, so no path under `/__spindle/` ever reaches one. A
  variable-only template such as `/{a}/{b}` stays served everywhere else. With the flag off,
  `/__spindle/*` answers `404`; with it on, the reserved routes answer.
- **Off means off.** Without `--observe` the surface holds no recorder: no clock read, no
  allocation, one `Option` branch per request.
- **Classes** are derived from the answer, not authored: `501` is `not-served`; `403` with
  `refused: "not granted"` is `not-granted`; any other answer carrying `refused` with `4xx` is
  `refused`, with `5xx` `failed`; no route or wrong method is `unrouted`; everything else,
  including a declared error outcome such as `410 already-done`, is `served`, since the model
  answered it.
- **Reserved requests are not recorded or counted**, so polling the panel does not move a count.
- **Resets do not clear** the log or any counter; Prometheus counters stay monotonic.
  `spindle_resets_total` counts them and `spindle_entity_rows` drops.
- **Durations** are measured on the listener thread from the request's head read to the answer
  handed back, so queueing behind a running command is included.
- **Panel build** uses a second Vite config and a small local plugin that inlines the emitted
  script and style into the HTML; no new runtime dependency.

## Stories

| order | story | depends on |
|---|---|---|
| 1 | `story:observe-ingress-model` | none |
| 2 | `story:ingress-request-log` | 1, `story:http-router-is-transport-free`, `story:outcome-scoped-refusal`, `story:fixed-caller-per-grant` |
| 3 | `story:ingress-prometheus-metrics` | 2 |
| 3 | `story:ingress-observe-panel` | 1, 2 |
| 4 | `story:external-answers-observed` | 2, 3 (metrics), 3 (panel), `story:fixed-caller-per-grant` |
| 5 | `story:live-topology` | 1, 2, 3 (metrics), 3 (panel), 4 |

The metrics and panel stories can run in parallel. They, the external-answers story and the topology story each add one arm
to the reserved-route dispatch in `crates/spindle-ingress/src/observe.rs`. Story 2 lays out all
four arms, so each overlap is one line; external-answers runs after the panel because both edit that file. The topology's multi-specification test waits for
`story:one-ingress-several-specifications` (draft) as an ignored test; the story does not depend
on it.

## Origin

The operator, 2026-11-06, relayed and condensed (not verbatim): "Spindle offers an ingress
including a router that understands incoming requests and outgoing responses, so we should be able
to have some mini Prometheus inside, or at least an endpoint returning metrics or logs. We want to
understand what's going on inside that runtime: runtime metrics or runtime tracing. If enabled, it
could come with a little UI that can be embedded, or the raw values accessed via an API on its own
routing."

A consumer's asks, 2026-11-06, condensed: an opt-in flag on `run`, off by default with no route
and no cost; `/__spindle/metrics` in Prometheus text; `/__spindle/requests` as a JSON ring buffer
with a filter for 501/unanswered; `/__spindle/ui` as one self-contained iframe-friendly page; JSON
stable enough for a one-line served/501/refused banner. Acceptance named by the consumer: with the
flag, the metrics parse as Prometheus text and counts match a scripted request sequence; without
it the routes answer `404`.

The operator, 2026-11-06, relayed by the coordinator, condensed: add `live-topology`, a live
topology and data-flow view from the ingress/router down through each served specification, the
external causes and how they were answered, and the emitted events and their channels. Edges
carry live numbers. It is served in the embeddable panel and as JSON at `/__spindle/topology`, and
spans several specifications behind one ingress. Same opt-in flag.

## Decided questions (coordinator, 2026-11-06)

1. CORS: none, no `Access-Control-Allow-Origin` header. The routes are same-origin, and a console
   on another origin proxies them, as the consumer already proxies its spindle routes.
2. A declared error outcome (`410 already-done`, `404 no-such-task`) counts as `served`, with
   `status_class="4xx"`, because the model answered it.
3. Reserving `/__spindle/` in ESS itself is out of scope; spindle refuses such a route at start.
   It is a possible ESS issue (an `ess-http/1` diagnostic), not a story here.
4. The flag is `--observe` on `run`. `deploy` keeps its own observe API, and the two commands
   share no flags.
5. The reserved prefix is matched before any specification route, flag on or off, so
   `/__spindle/*` never reaches a specification route and an off run answers `404` there.
   Story `ingress-request-log` tests it with a variable-only template.

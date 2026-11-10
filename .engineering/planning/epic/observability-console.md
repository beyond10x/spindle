---
format: aep.planning-md/3
id: epic:observability-console
kind: epic
status: draft
title: An observability console shipped as an image
summary: One stateless image whose pages are generated from a Prometheus URL and Spindle model URLs, with no dashboards to import.
relations:
- serves: vision:spindle
- informed_by: epic:runtime-observability
revision: 2
---
# An observability console shipped as an image

## Outcome

One image, `ghcr.io/beyond10x/spindle-console`, runs a stateless, non-root web console on one port.
Its inputs are one Prometheus URL and one or more Spindle model URLs (a served run, a component
process or a front ingress answering `/api/model`, `/__spindle/topology` and
`/__spindle/metrics/families`). From those alone it generates its pages:

- system, domain, component, command, view, event, external cause and store, each with rate,
  errors and duration percentiles (p50, p95, p99) read from Prometheus;
- a request page listing a model's recent requests, and its span tree where traces are on.

No dashboard to import, no per-model configuration, no dashboard tool to set up: a new model behind a
URL gets its pages with no change to the console. The console never deploys or configures
Prometheus; the operator's Prometheus scrapes what `epic:kubernetes-target` marks.

## Why

Timo, 2026-11-10, relayed by the requesting session (verbatim excerpts; one product name replaced
by "[a dashboard tool]", per the public-repository rule):

> "a own UI basically coming with Spindle … some OCI thing … which holds a really nice web UI.
> Basically replacing [a dashboard tool] for the Prometheus part"

> "Prometheus is not something which is coming with Spindle"

## Decision: a second application, not the panel grown up

The console is a second application with its own crate, binary and image. It reuses the `ui/`
toolchain, the generated observe types (`ui/src/api/generated/`) and the SVG components
(`ui/src/components/Sparkline.vue`, `Bars.vue`), as a third Vite build beside `vite.config.ts` and
`vite.panel.config.ts`.

Why not the embedded panel (`ui/src/panel/`, built into `ui/dist-panel/panel.html`, answered at
`GET /__spindle/ui`):

1. The panel reads one run, same-origin, from that run's own ring and counters
   (`ui/src/panel/main.ts:1-2`); the console reads history from Prometheus and several models at
   once. Range queries and percentiles over time are not something one process holds.
2. The panel is compiled into the runtime binary (`crates/spindle-cli/build.rs`); a Prometheus
   client and a forwarder for third-party URLs do not belong in the binary that runs the model.
3. The reserved routes answer no `Access-Control-Allow-Origin` (`epic:runtime-observability`,
   decided question 1), and the console must read them from another origin. A small server that
   forwards read-only `GET`s to its configured URLs is the way across, and the panel has no server.
4. The panel must stay one inlined page an iframe can hold; the console has many pages and routes.

Why not the deploy UI (`ui/src/App.vue`): it reads the control process's token-guarded stream
(`crates/spindle-control/src/server.rs:251`) of one deployment, and runs only under `deploy`.

Naming: `spindle.console` and the console routes (`/api/model`, `/api/events`, `/api/reset`,
`crates/spindle-ingress/src/console.rs`) are the model console a served run already answers. The
new application is "the observability console" everywhere, so the two are not confused.

## What exists at tag 0.8.0

| fact | where |
|---|---|
| No image of any kind is built or published; the release ships 4 tarballs and `SHA256SUMS` | `.github/workflows/release.yml:160-169`, `release.yml:264-298` |
| The topology route answers `404` until `story:live-topology` (active) lands | `crates/spindle-ingress/src/observe.rs:64` |
| The family description type exists, served by no route | `ess/domains/observe.yaml:429-452` |
| The model is served at `GET /api/model` | `crates/spindle-cli/src/cli.rs:113-114` |

## Domain first (guardrail 7)

A console page, the console's sources and what a page shows have no home in any ESS document of
this repository. `story:console-pages-specified` drafts them in `spindle.observe` before any other
story of this epic is implemented.

## Stories, in delivery order

| order | story | depends on |
|---|---|---|
| 1 | `story:console-pages-specified` | `story:observe-label-vocabulary` |
| 2 | `story:console-server` | 1; needs Timo's acceptance |
| 3 | `story:console-model-pages` | 1, 2, `story:metric-families-route`, `story:store-and-kernel-durations`, `story:view-read-families`, `story:live-topology` |
| 4 | `story:console-request-page` | 3 |
| 5 | `story:console-span-waterfall` | 4, `story:trace-nouns-specified` (its fixture; no run has to serve traces) |

The request list reads the ring that exists today; the waterfall waits only for the trace nouns
and their fixture. The epic carries no `depends_on` of its own: the story edges carry the order.

The image is published by `story:console-image-published` in `epic:kubernetes-target`, which owns
the release workflow. Across the four epics the console comes after the metrics of
`epic:runtime-observability-depth` and the PostgreSQL store of `epic:split-run`, and before the
split run.

## Acceptance of the epic

Against a recorded Prometheus (a fixture server answering the recorded query answers) and two
different models (`examples/rally/ess` and a second neutral fixture), every page renders, with no
code or configuration change between the two models.

## Not in this epic

- Prometheus itself, its configuration, alert rules, or any collector.
- State: the console stores nothing, no user accounts, no saved views.
- Writing to a model: the console forwards `GET` only.

## Needs Timo's acceptance

`story:console-server`: a second shipped application. `story:console-image-published`: a second
shipped image.

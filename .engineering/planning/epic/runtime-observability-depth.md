---
format: aep.planning-md/3
id: epic:runtime-observability-depth
kind: epic
status: draft
title: 'Runtime observability in depth: every level timed, described and traceable'
summary: Duration histograms per level, view and queue families, one label vocabulary from the model, a family description route and opt-in traces, on run, deploy and every component process.
relations:
- serves: vision:spindle
- informed_by: epic:runtime-observability
- informed_by: epic:observe
revision: 3
---
# Runtime observability in depth: every level timed, described and traceable

## Outcome

A served run, the control process of `deploy` and, once `run` is split, every component process
answer the reserved routes, and what they expose reaches below the request. A served run and every
component process answer `/__spindle/metrics`, `/__spindle/metrics/families`, `/__spindle/requests`
and `/__spindle/topology`; the control process answers `/__spindle/metrics` and
`/__spindle/metrics/families` for its deployment.

- a duration histogram at every level a request passes through: ingress, command decision, store
  read and append, view evaluation, binding delivery, the answer to an external cause and the lag
  of a periodic tick; admission once per component at start;
- view families: reads, duration and rows returned, per view;
- queue families: envelopes waiting in the outbox and their age, envelopes sent and not yet
  acknowledged per binding, ticks pending per periodic binding;
- one label vocabulary read from the model, bounded by it: `domain`, `component`, `kind`
  (`entity`, `command`, `view`, `event`, `external`, `binding`) and `name`; no label ever carries an
  entity identity, an actor, a path value or any other value the model does not declare;
- a machine-readable description of every family, `GET /__spindle/metrics/families`, so a reader
  builds its queries from the description instead of from a list it was given;
- opt-in traces: one span tree per request, its trace id in the request log, export over OTLP, and
  a trace that follows an event from one component process to another.

This continues `epic:runtime-observability` (active), whose stories gave a served run the request
log, 14 families, the panel and the topology.

## Why

Timo, 2026-11-10, relayed by the requesting session (verbatim excerpts):

> "some visibility or observability epic … tracking all kind of things like the handlers, the
> inputs, events, latencies on all levels … it shall be generic … later automatically it will set
> this up across all the different kinds and domains."

"Generic" and "automatically … across all the different kinds and domains" is what the label
vocabulary and the family description are for: the console of `epic:observability-console` builds
every page from them with no per-model configuration.

## Acceptance

- On rally served by `run --observe`, `/__spindle/metrics/families` lists a family at every level
  the vocabulary names except `deployment`.
- Every family `/__spindle/metrics` exposes is listed by `/__spindle/metrics/families`.
- After a scripted sequence on rally with distinct entity identities and actors, no label value of
  `/__spindle/metrics` equals one of those identities or actors.
- With rally split into three `run --component` processes, each process answers the four routes
  for its own component.
- `deploy`'s control process answers `/__spindle/metrics` with one series per placed instance.
- With `--observe-traces`, an `OpenMatch` request's record carries a trace id whose span tree the
  run answers (once Timo has accepted `story:trace-nouns-specified`).

## What exists at tag 0.8.0

| fact | where |
|---|---|
| 14 families, in the order `ess/fixtures/observe/metrics/families.json` lists them | `crates/spindle-ingress/src/observe/metrics.rs:38-135` |
| Two duration histograms: the request, with no labels, and the kernel's execution per command, buckets only (no `_sum`) | `metrics.rs:55-59`, `metrics.rs:74-78`, `metrics.rs:12-14` |
| The command duration is measured around `spindle_kernel::execute_refusing` | `crates/spindle-host/src/host.rs:1339`, `host.rs:1358` |
| The request duration is measured on the listener thread | `crates/spindle-ingress/src/listener.rs:314` |
| Fixed buckets, 12 bounds in microseconds | `crates/spindle-telemetry/src/counters.rs:15` |
| The store is wrapped by `CountingStore`, which counts rows and bytes but times nothing | `crates/spindle-telemetry/src/kernel.rs:49-60` |
| The description type already exists, `spindle.observe.MetricFamily` and `MetricFamilies`, but no route serves it | `ess/domains/observe.yaml:429-452`; `crates/spindle-ingress/src/observe.rs:56-68` (four routes, no families) |
| Envelopes not yet sent are counted, but not exposed | `crates/spindle-channel/src/endpoint.rs:295-298` (`Sender::staged`) |
| Admission runs once, at start, not per request | `crates/spindle-cli/src/cli.rs:84-85` |
| An external cause is answered by a decider in the process (world, decisions file, default), not by a call over a network | `crates/spindle-host/src/decisions.rs:1`, `metrics.rs:129-133` |
| `deploy`'s control process serves its own API and no `/__spindle/` route | `crates/spindle-control/src/server.rs:239-262` |
| No tracing: no tracing or OpenTelemetry crate in the lock file; `epic:runtime-observability` lists "OTLP export, distributed tracing across components, per-envelope traces" as out | `Cargo.lock`; `epic:runtime-observability` § Scope, Out |
| A downstream model, measured on 0.8.0 (2026-11-10): `spindle_commands_total` is labelled by qualified command name, and every command and outcome has a zero series from the start: 12,225 series and 1,198,629 bytes per scrape. The label vocabulary (story 1) and the families route (story 2) state a series bound per family | reported by the downstream deployment |

## Domain first (guardrail 7)

- The label vocabulary and the family description have a home: `spindle.observe` already declares
  `MetricFamily` (`ess/domains/observe.yaml:429`). `story:observe-label-vocabulary` extends it before
  any family story is implemented.
- A trace and a span have no home in any ESS document of this repository. They are drafted in
  `spindle.observe` by `story:trace-nouns-specified` before `story:request-span-tree`,
  `story:otlp-trace-export` or `story:trace-crosses-processes` is implemented.

## Stories, in delivery order

| order | story | depends on |
|---|---|---|
| 1 | `story:observe-label-vocabulary` | none |
| 2 | `story:metric-families-route` | 1 |
| 3 | `story:store-and-kernel-durations` | 1, 2 |
| 4 | `story:view-read-families` | 1, 2, 3 |
| 4 | `story:binding-external-timer-durations` | 1, 2, 3 |
| 5 | `story:queue-families` | 1, 2, 4 (binding durations) |
| 5 | `story:control-process-metrics` | 1, 2 |
| 6 | `story:trace-nouns-specified` | 1, `story:console-pages-specified`; needs Timo's acceptance |
| 7 | `story:request-span-tree` | 6, 3, 4, 5 (queue); needs Timo's acceptance |
| 8 | `story:otlp-trace-export` | 7; needs Timo's acceptance |
| 9 | `story:component-process-observe` | 2, `story:run-one-component`, `story:live-topology` |
| 10 | `story:trace-crosses-processes` | 7, `story:run-one-component`; needs Timo's acceptance |
| 1 | `story:refusal-reason-bounded` | none; reported 2026-11-10: a refused command's logged reason holds the caller's value, unbounded |

Across the four epics drafted on 2026-11-10 the order is: the Spindle image
(`epic:kubernetes-target`), then stories 1-5 here, then the PostgreSQL store (`epic:split-run`),
then the console (`epic:observability-console`), then the split run with stories 9 and 10 here,
then the Kubernetes renderer. Stories 6-8 follow once Timo has accepted story 6.

Shared files: stories 2-5 and 7 each add to `crates/spindle-ingress/src/observe/metrics.rs` and
`ess/fixtures/observe/metrics/families.json`; the `depends_on` edges put them in a line. Story 6
and `story:console-pages-specified` both edit `ess/domains/observe.yaml` and the generated types,
so 6 runs after it. `story:live-topology` (active) also edits `ess/domains/observe.yaml` and
`crates/spindle-ingress/src/observe.rs`, so stories 1 and 2 do not run beside it.

## Not in this epic

- A dashboard or a page: `epic:observability-console`.
- Deploying or configuring Prometheus, or any collector: Spindle exposes, others scrape.
- Persistence of the request log, the samples or the traces across a restart.
- Labels per entity instance, per actor or per path value.
- The reserved routes on the front ingress of `story:front-ingress`.

## Needs Timo's acceptance

`story:trace-nouns-specified` reopens the "no tracing" exclusion of `epic:runtime-observability`;
`story:request-span-tree`, `story:otlp-trace-export` and `story:trace-crosses-processes` rest on
it. `story:trace-crosses-processes` is the distributed tracing across components that epic
excluded.

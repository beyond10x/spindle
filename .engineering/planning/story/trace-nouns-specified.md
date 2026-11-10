---
format: aep.planning-md/3
id: story:trace-nouns-specified
kind: story
status: draft
title: Traces are in scope, and a trace and a span are specified in spindle.observe
tags:
- needs-timo-acceptance
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability-depth
- depends_on: story:observe-label-vocabulary
- informed_by: epic:runtime-observability
- depends_on: story:console-pages-specified
scope:
- confidence: cited
  path: crates/spindle-observe-api
- confidence: cited
  path: docs/design/observe.md
- confidence: cited
  path: ess/domains/observe.yaml
- confidence: inferred
  path: ess/fixtures/observe/traces
- confidence: cited
  path: ui/src/api/generated
revision: 4
---
# Traces are in scope, and a trace and a span are specified in spindle.observe

**Needs Timo's acceptance.** This story reopens an exclusion of `epic:runtime-observability`, whose
Scope, Out, lists "OTLP export, distributed tracing across components, per-envelope traces (§ 9 of
the observe design)", and `docs/design/observe.md` § 9 ("tracing individual envelopes end to end
… is a later story"). Timo asked on 2026-11-10 for "latencies on all levels"; whether that
includes traces is his call, not this plan's.

## Outcome

- `docs/design/observe.md` § 9 records that per-request traces are in scope, opt-in, and how they
  relate to the families (a span per level the histograms time).
- `spindle.observe` declares the nouns (guardrail 7: no ESS document of this repository has them):
  a trace (its id and its root span), a span (id, parent, level, `kind` and `name` from the label
  vocabulary, start, duration, result) and the trace answer a run serves; `RequestRecord` gains an
  optional `trace_id`.
- Fixtures of a rally request's span tree validate against the generated schema.

## Acceptance

- `ess specify validate --path ess` accepts the trace and span nouns.
- `ess specify validate --path ess` accepts a `RequestRecord` fixture without a `trace_id` and one
  with it.
- `ess/fixtures/observe/traces/` holds one span tree of a rally `OpenMatch` request that validates
  against the generated schema.
- `task check:the_generated_types_have_no_drift` passes.
- `docs/design/observe.md` § 9 no longer lists per-request tracing as later.

## Scope

- `ess/domains/observe.yaml` (`RequestRecord`, lines 304-361; new nouns).
- `ess/fixtures/observe/traces/` (new), `crates/spindle-observe-api/`, `ui/src/api/generated/`.
- `docs/design/observe.md` § 9.

## Shared files

`story:console-pages-specified` also edits `ess/domains/observe.yaml`, `crates/spindle-observe-api/`
and `ui/src/api/generated/`; this story runs after it.

---
format: aep.planning-md/3
id: story:trace-crosses-processes
kind: story
status: draft
title: A trace follows an event from one component process to another
tags:
- needs-timo-acceptance
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability-depth
- depends_on: story:request-span-tree
- depends_on: story:run-one-component
- informed_by: epic:runtime-observability
scope:
- confidence: cited
  path: crates/spindle-channel/src/model.rs
- confidence: cited
  path: crates/spindle-channel/src/tcp.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli
- confidence: inferred
  path: crates/spindle-ingress/src/observe/traces.rs
- confidence: cited
  path: crates/spindle-telemetry/src/port.rs
revision: 3
---
# A trace follows an event from one component process to another

**Needs Timo's acceptance**: this is the distributed tracing across components that
`epic:runtime-observability` excluded ("distributed tracing across components, per-envelope
traces"). It rests on `story:trace-nouns-specified`, which reopens that exclusion.

## Outcome

In a split run with traces on, each envelope sent over `channel = tcp` carries the trace id and the
id of the span that emitted its event. The receiving process opens its `binding` span under that
parent, so the delivery and the command it invokes belong to the request's trace, though they are
kept by another process. Each process answers `/__spindle/traces/{trace_id}` with the spans it
holds; joining them is the reader's work (the OTLP collector, or the console).

## Acceptance

- `a_trace_crosses_processes`: with rally split into three processes and traces on, after
  `OpenMatch` at the table, a player's process answers the request's `trace_id` with a `binding`
  span whose parent is a span the table's process holds.
- `an_envelope_without_trace_context_opens_no_trace`: with traces off at the sender, the receiver
  records no span for the delivery.

## Scope

- `crates/spindle-channel/src/model.rs` (`Envelope`: trace id and parent span id),
  `crates/spindle-channel/src/tcp.rs` (frames).
- `crates/spindle-telemetry/src/port.rs` (a record that names a remote parent).
- `crates/spindle-ingress/src/observe/traces.rs`.
- `crates/spindle-cli/tests/`.

---
format: aep.planning-md/3
id: story:request-span-tree
kind: story
status: draft
title: An observed run keeps one span tree per request, on request
tags:
- needs-timo-acceptance
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability-depth
- depends_on: story:trace-nouns-specified
- depends_on: story:store-and-kernel-durations
- depends_on: story:binding-external-timer-durations
- depends_on: story:view-read-families
- depends_on: story:queue-families
scope:
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe.rs
- confidence: inferred
  path: crates/spindle-ingress/src/observe/traces.rs
- confidence: cited
  path: crates/spindle-telemetry/src/port.rs
revision: 4
---
# An observed run keeps one span tree per request, on request

**Needs Timo's acceptance**: it rests on `story:trace-nouns-specified`, which reopens the "no
tracing" exclusion.

## Outcome

With `--observe-traces` (beside `--observe`), every request recorded in the ring gets a trace. Its
`trace_id` is in the request's record at `/__spindle/requests`, and
`GET /__spindle/traces/{trace_id}` answers its span tree while the request is in the ring. Without
the flag nothing is recorded for spans and the trace route answers `404`.

The spans of one request:

- the root: the ingress span, from the request's head read to its answer;
- a `command` span per command the request ran, the routed one and each one a binding invoked;
- under a command span: a `store` span per store read and per append, and an `external` span per
  external cause it asked;
- a `binding` span per envelope delivered, under the command span that emitted the event, holding
  the `command` span of the command it invoked;
- a `view` span for a view read.

Spans are built from the telemetry records the histogram stories emit, each with its start
instant: a collector is armed on the host's thread for the length of one request and gathers the
records emitted meanwhile. This story does not edit `crates/spindle-host/src/host.rs`,
`crates/spindle-channel/src/endpoint.rs` or `crates/spindle-telemetry/src/kernel.rs`; it runs
after every story that does and adds a record.

## Acceptance

- `an_open_match_has_its_spans`: an `OpenMatch` request on rally answers a tree whose root is the
  ingress span with one `command` span for `OpenMatch` under it.
- `store_spans_sit_under_their_command`: the `OpenMatch` command span holds one `store` span per
  read and per append the command made.
- `each_delivery_has_a_binding_span`: the tree has one `binding` span per envelope the request's
  events were delivered as, each holding the `command` span it invoked.
- `the_record_names_its_trace`: the request's record carries the `trace_id` the trace route
  answers.
- `without_traces_no_trace_id`: with `--observe` and without `--observe-traces`, records carry no
  `trace_id`.
- `without_traces_the_route_answers_404`: with `--observe` and without `--observe-traces`, the
  trace route answers `404`.
- `the_root_fits_the_request`: the root span's duration is at most the record's `duration_us`.
- `children_nest_in_their_parent`: every span starts no earlier than its parent and ends no later.

## Scope

- `crates/spindle-telemetry/src/port.rs` (`Record`, `Sink`: the start instant, the collector).
- `crates/spindle-ingress/src/observe.rs` (`Recorder::record` line 462; `Reserved` lines 56-68).
- `crates/spindle-ingress/src/observe/traces.rs` (new: the tree, the trace route).
- `crates/spindle-cli/src/cli.rs` (`--observe-traces`, beside `observe` at line 215),
  `crates/spindle-cli/src/serve.rs`.

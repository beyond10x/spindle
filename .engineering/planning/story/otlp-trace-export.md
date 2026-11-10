---
format: aep.planning-md/3
id: story:otlp-trace-export
kind: story
status: draft
title: An observed run exports its spans over OTLP
tags:
- needs-timo-acceptance
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability-depth
- depends_on: story:request-span-tree
scope:
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe/metrics.rs
- confidence: inferred
  path: crates/spindle-ingress/src/observe/otlp.rs
revision: 3
---
# An observed run exports its spans over OTLP

**Needs Timo's acceptance**: it rests on `story:trace-nouns-specified`, which reopens the "no
tracing" exclusion, and it adds an outbound connection a run makes on its own.

## Outcome

With `--otlp <url>` beside `--observe-traces`, a run sends each finished span tree to an OTLP/HTTP
endpoint (`/v1/traces`, JSON encoding), in batches, off the host's thread. An unreachable endpoint
drops batches and counts them; it never blocks or fails a request. The resource attributes name
the specification, the component and the Spindle version; span attributes use the label
vocabulary.

## Acceptance

- `spans_reach_an_otlp_endpoint`: against a test server, after one `OpenMatch` request the server
  received one trace whose spans equal the tree `/__spindle/traces/{trace_id}` answers.
- `an_unreachable_endpoint_drops_and_counts`: with the endpoint down, requests answer as without
  `--otlp`, and a dropped-batches counter grows.
- `--otlp` without `--observe-traces` exits 1 before listening, naming the missing flag.

## Scope

- A new module in `crates/spindle-ingress/src/observe/` (the exporter), using the `hyper` client
  already in the lock file.
- `crates/spindle-cli/src/cli.rs`, `crates/spindle-cli/src/serve.rs`.
- `crates/spindle-ingress/src/observe/metrics.rs` (the dropped-batches counter).

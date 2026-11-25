---
format: aep.planning-md/3
id: story:wasi-externals-over-http
kind: story
status: draft
title: A WASI module answers external causes over HTTP
relations:
- serves: vision:spindle
- decomposes: epic:wasi-server-module
- depends_on: story:wasi-http-serves-a-specification
revision: 1
---
# A WASI module answers external causes over HTTP

## Outcome

An `external:` cause the environment binds to an HTTP call is answered by one outbound request
through `wasi:http/outgoing-handler`, as `run` answers it natively (`crates/spindle-cli/src/calls.rs`),
the decisions file and the answer default beneath it.

## Acceptance

- `an_external_cause_is_answered_by_a_call_under_wasmtime` — a stub server on loopback answers
  the call; the command's outcome follows its answer; a failed call answers as natively.

## Depends on

- `story:wasi-http-serves-a-specification`.

## Scope

- The component crate (inferred).

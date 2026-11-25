---
format: aep.planning-md/3
id: story:wasi-state-survives-instances
kind: story
status: draft
title: State survives the instances a WASI server makes
relations:
- serves: vision:spindle
- decomposes: epic:wasi-server-module
- depends_on: story:wasi-http-serves-a-specification
revision: 1
---
# State survives the instances a WASI server makes

## Outcome

**This story is the epic's design decision.** `wasmtime serve` may instantiate the component per
request, so state kept in the instance's memory does not survive from one request to the next.
The story decides, with measurements, where state lives between instances — a store the WASI host
provides (`wasi:keyvalue`, a preopened directory), a long-lived instance the host reuses, or state
serialised out and in per request — and what each costs per request, and records the decision in
`docs/design/spindle.md` §11.

## Acceptance

- A decision record in §11 names the chosen place, the two rejected, and the per-request cost
  measured for each on the review fixture.
- `a_write_is_seen_by_the_next_request` — under `wasmtime serve`, a command then a view in two
  requests: the view lists the row.

## Depends on

- `story:wasi-http-serves-a-specification`.

## Scope

- `docs/design/spindle.md`, the component crate (inferred).

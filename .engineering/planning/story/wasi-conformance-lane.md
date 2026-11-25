---
format: aep.planning-md/3
id: story:wasi-conformance-lane
kind: story
status: draft
title: The conformance suite runs against a WASI module
relations:
- serves: vision:spindle
- decomposes: epic:wasi-server-module
- depends_on: story:wasi-http-serves-a-specification
revision: 1
---
# The conformance suite runs against a WASI module

## Outcome

`task check` runs the suite ESS synthesises from `ess/` against the WASI component over HTTP,
the same scenarios executing and passing as `check:conformance` holds in `conformance.toml`.

## Acceptance

- `check:wasi_conformance` — the counts equal `conformance.toml`'s, within the 120 s test-step
  budget, each scenario bounded at 60 s.

## Depends on

- `story:wasi-http-serves-a-specification`.

## Scope

- `Taskfile.yml`, `crates/spindle-conformance` (inferred).

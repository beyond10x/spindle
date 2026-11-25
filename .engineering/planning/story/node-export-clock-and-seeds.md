---
format: aep.planning-md/3
id: story:node-export-clock-and-seeds
kind: story
status: draft
title: The Node export runs on a clock and seeds its host program controls
relations:
- serves: vision:spindle
- decomposes: epic:browser-host
- depends_on: story:node-export-serves-a-specification
revision: 1
---
# The Node export runs on a clock and seeds its host program controls

## Outcome

A consumer starts an instance at a given instant, sets and advances its clock (or hands it a
`now()` callback), and periodic bindings fire as the clock passes their times: the instance calls
`Host::advance` (`crates/spindle-host/src/host.rs`), which no wasm host calls today. Identities and
generated values are minted from a seed in the configuration (`spindle_kernel::Seeded`, as a reset
world's `mint` does), and any randomness the host uses is seeded too, so two runs of one script
answer the same bytes.

## Acceptance

- `advancing the clock fires a periodic binding` — a fixture with a periodic binding: no tick
  before its interval, one after `advance` passes it, as `ManualClock` drives it natively.
- `a seeded instance answers the same bytes twice` — two instances created with one mint seed
  answer one request script with identical bodies, minted identities included.
- `a clock start sets now` — a guard ordering against now is decided at the configured instant.

## Depends on

- `story:node-export-serves-a-specification`.

## Scope

- `crates/spindle-embed/`, `crates/spindle-node/` (inferred).

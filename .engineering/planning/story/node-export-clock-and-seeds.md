---
format: aep.planning-md/3
id: story:node-export-clock-and-seeds
kind: story
status: draft
title: The Node export runs on a clock and seeds its host program controls
relations:
- serves: vision:spindle
- depends_on: story:node-export-serves-a-specification
- decomposes: epic:node-export
- depends_on: story:node-export-fixed-actor
scope:
- confidence: inferred
  path: crates/spindle-embed
- confidence: inferred
  path: crates/spindle-node
revision: 4
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
- `setting the clock moves now` — after `setClock(t)`, a guard ordering against now is decided
  at `t`, before and after a periodic binding's time.
- `a different seed mints different identities` — two instances with different mint seeds answer
  the same script with different minted identities, and any other randomness the host draws (the
  random seed) repeats with its seed and differs without it.

## Depends on

- `story:node-export-serves-a-specification`, and `story:node-export-fixed-actor`: both extend
  the configuration `create` reads and `crates/spindle-node/src/lib.rs`.

## Scope

- `crates/spindle-embed/`, `crates/spindle-node/` (inferred).

---
format: aep.planning-md/3
id: epic:m1-m3-rally-in-three-vms
kind: epic
status: draft
title: 'M1-M3: rally in three microVMs'
relations:
- serves: vision:spindle
- depends_on: epic:m0-run-in-process
revision: 1
---
# M1–M3 — rally in three microVMs

## Outcome

The `rally` example system (`examples/rally/`, design §13) runs as three components — one table,
two players — first as three processes, then each in its own microVM, communicating only through
the bindings the specification declares.

## Stories

- `story:periodic-bindings-fire` — the timer capability drives periodic bindings.
- `story:bindings-cross-components` — the channel capability, `in-process` and `tcp`.
- `story:rally-specification` — the example system in ESS.
- `story:rally-runs-as-three-processes` — M2.
- `story:rally-runs-in-three-microvms` — M3.

## Not in this epic

A deploy service, a bundle registry, cells, leases and snapshots (design §12, M4–M5).

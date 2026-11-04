---
format: aep.planning-md/3
id: architecture-design:spindle
kind: architecture-design
status: draft
title: Spindle architecture
relations:
- serves: vision:spindle
revision: 2
---
# Spindle architecture

The design is `docs/design/spindle.md`; the observe extension is `docs/design/observe.md`. This
record names the parts and the boundaries between them.

## Parts

Checked against `main` at `b9b0cd9` (plan review, 2026-11-04).

| part | crate | depends on | state |
|---|---|---|---|
| loader | `spindle-spec` | `ess-domain`, `ess-compiler` at the pinned tag | shipped |
| capabilities and admission | `spindle-capability` | the IR | shipped |
| executor | `spindle-kernel` | the IR, a store port (memory, SQLite) | shipped |
| timers | `spindle-timer` | the IR, a `Clock` port | shipped |
| channels and outbox | `spindle-channel` | the IR; in-process and TCP | shipped |
| host: every component in one process, views | `spindle-host` | kernel, timer, channel, telemetry | shipped |
| deployment target contract, process launcher, host-side sampling | `spindle-target` | `spindle-capability`; models `spindle.placement` | shipped |
| QEMU `microvm` launcher | `spindle-qemu` | `spindle-target` | shipped |
| Firecracker launcher | `spindle-firecracker` | `spindle-target` | shipped |
| telemetry port, counters, catalogue | `spindle-telemetry` | the IR | shipped |
| observe API types (generated, outside the workspace) | `spindle-observe-api` | `ess/` `spindle.observe` | shipped |
| observe UI (Vue, fixtures only) | `ui/` | the generated TypeScript types | shipped |
| site generator | `spindle-docs` | `spindle-cli` | shipped |
| command line | `spindle-cli` (`b10x-spindle`) | all of the above | shipped |
| telemetry transport | `spindle-telemetry`, the launchers | — | planned, `story:telemetry-transport` |
| control process | not yet named | — | planned, `story:control-process-serves-the-api` |
| HTTP ingress | not yet named | — | planned, `story:run-over-http-ingress` |
| conformance target | `spindle-conformance` (new) | — | planned, `story:spindle-conforms-to-its-specification` |

## Boundaries

- ESS is read only through its crates. Spindle never parses ESS source itself and never
  re-implements a guard.
- Capabilities are derived from the IR (`derive`). A specification and a bundle never list them.
- An environment (`spindle-environment/1`, TOML) maps capabilities to implementations in order of
  preference; admission picks the first.
- A step is deterministic: `(state, command) → (state, events, effects)`. Effects leave through an
  outbox after the state is durable.
- A launcher passes an instance its identity, component, listen address, peers and settings as
  `SPINDLE_*` variables (`crates/spindle-target/src/process.rs`).

## Open

- Whether an overlay document carries deployment defaults (design §11).

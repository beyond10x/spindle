---
format: aep.planning-md/3
id: architecture-design:spindle
kind: architecture-design
status: draft
title: Spindle architecture
relations:
- serves: vision:spindle
revision: 1
---
# Spindle architecture

The design is `docs/design/spindle.md`. This record names the parts and the boundaries between
them.

## Parts

| part | crate | depends on |
|---|---|---|
| loader | `spindle-spec` | `ess-domain`, `ess-compiler` at the pinned tag |
| capabilities and admission | `spindle-capability` | `spindle-spec` (tests), the IR |
| executor (planned) | `spindle-kernel` | the IR, a store port, an outbox |
| bindings and timers (planned) | `spindle-host` | the executor, channel and timer ports |
| command line | `spindle-cli` (`b10x-spindle`) | all of the above |

## Boundaries

- ESS is read only through its crates. Spindle never parses ESS source itself and never
  re-implements a guard.
- Capabilities are derived from the IR (`derive`). A specification and a bundle never list them.
- An environment (`spindle-environment/1`, TOML) maps capabilities to implementations in order of
  preference; admission picks the first.
- A step is deterministic: `(state, command) → (state, events, effects)`. Effects leave through an
  outbox after the state is durable.

## Open

- Whether an overlay document carries deployment defaults (design §11).

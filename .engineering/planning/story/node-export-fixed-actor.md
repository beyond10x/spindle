---
format: aep.planning-md/3
id: story:node-export-fixed-actor
kind: story
status: draft
title: The Node export sends commands as fixed actors
relations:
- serves: vision:spindle
- decomposes: epic:browser-host
- depends_on: story:node-export-serves-a-specification
revision: 1
---
# The Node export sends commands as fixed actors

## Outcome

A consumer's configuration names fixed actors, each with its caller attributes, as `run`
takes them (`--fixed-actor`, `--fixed-caller`): each served operation is sent or read as the first
fixed actor its grant names (`Surface::fixing_callers`). An actor the specification does not
declare, or a caller its binding refuses, is refused by `create` with the reason `run` gives.

## Acceptance

- `a fixed actor sends each command as run does` (Node lane `check:node_serves_a_specification`) —
  a fixture with a granted command answers as the native loopback listener answers it with the same
  `--fixed-actor`/`--fixed-caller`, recorded natively.
- `create refuses an undeclared fixed actor`, with `run`'s reason.

## Depends on

- `story:node-export-serves-a-specification`.

## Scope

- `crates/spindle-embed/`, `crates/spindle-node/` (inferred).

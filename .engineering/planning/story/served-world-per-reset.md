---
format: aep.planning-md/3
id: story:served-world-per-reset
kind: story
status: active
title: A served run loads a world per reset
relations:
- serves: vision:spindle
- depends_on: story:scenario-establish-steps
- depends_on: story:served-run-answers-from-decisions
scope:
- confidence: inferred
  path: crates/spindle-cli/src/decisions.rs
- confidence: inferred
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-ingress/src/console.rs
- confidence: cited
  path: crates/spindle-ingress/src/listener.rs
- confidence: inferred
  path: crates/spindle-kernel/src/mint.rs
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T00:36:51Z", actor: "agent:b10x-chef-orchestrator", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-06T00:36:51Z", actor: "agent:b10x-chef-orchestrator", revision: 5}
---
# A served run loads a world per reset

## Outcome

`POST /api/reset` may carry a world: seed rows, an external decision table and an identity-mint
seed. The run empties every store as today (`crates/spindle-ingress/src/console.rs`, `reset`),
then writes each seed row into the store of the component that owns its entity through
`Host::establish` (from `story:scenario-establish-steps`), and answers external causes from the
given decision table (the decisions file of `story:served-run-answers-from-decisions`) until the
next reset. With a mint seed, identities are minted from it instead of from `UuidV4`
(`crates/spindle-kernel/src/mint.rs`), so the same requests after the same reset produce the same
identities. A reset with an empty body behaves as today.

## Acceptance

- `a_reset_with_seed_rows_serves_them` — a reset carrying two rows of an entity in a non-initial
  state: its view lists both, and a command guarded on that state runs against them.
- `a_reset_with_a_decision_table_answers_from_it` — a reset carrying a table that decides an
  external cause: the next request to that command takes the decided branch; a second reset with a
  different table takes the other branch.
- `a_seeded_mint_repeats_its_identities` — two runs, each reset with the same mint seed and sent
  the same creating requests, answer the same identities.
- `a_reset_without_a_body_empties_as_today` — an empty reset answers `{"emptied": [...]}` and
  leaves no rows.
- `a_world_that_does_not_validate_is_refused_and_changes_nothing` — a seed row for an undeclared
  entity, or a table naming an undeclared cause, is refused `400` and the stores keep their rows.

## Origin

Requested by a consumer that serves a specification over HTTP in memory as a test backend for a
browser UI suite, 2026-11-06.

## Scope

- `crates/spindle-ingress/src/console.rs` — cited: `reset`, which takes no body today.
- `crates/spindle-ingress/src/listener.rs` — cited: the console reset dispatch.
- `crates/spindle-host/src/host.rs` — inferred: reset with a world, establish, a swappable mint.
- `crates/spindle-kernel/src/mint.rs` — inferred: a seeded `Mint`.
- `crates/spindle-cli/src/serve.rs` — inferred: wiring the decisions into a served run.
- `crates/spindle-cli/src/decisions.rs` — inferred: reading a decision table from a request body.

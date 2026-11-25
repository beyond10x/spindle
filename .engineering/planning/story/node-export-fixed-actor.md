---
format: aep.planning-md/3
id: story:node-export-fixed-actor
kind: story
status: implemented
title: The Node export sends commands as fixed actors
relations:
- serves: vision:spindle
- depends_on: story:node-export-serves-a-specification
- decomposes: epic:node-export
scope:
- confidence: cited
  path: AGENTS.md
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: README.md
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: cited
  path: crates/spindle-embed/src/lib.rs
- confidence: cited
  path: crates/spindle-ingress/src/lib.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
- confidence: cited
  path: crates/spindle-node/Cargo.toml
- confidence: cited
  path: crates/spindle-node/src/export.rs
- confidence: cited
  path: crates/spindle-node/src/lib.rs
- confidence: cited
  path: crates/spindle-node/tests/fixtures/fixed-actor-exchanges.json
- confidence: cited
  path: crates/spindle-node/tests/node/fixed-actor.test.mjs
- confidence: cited
  path: crates/spindle-node/tests/spindle-node/fixed_actor.rs
- confidence: cited
  path: crates/spindle-node/tests/spindle-node/listener.rs
- confidence: cited
  path: crates/spindle-node/tests/spindle-node/main.rs
- confidence: cited
  path: crates/spindle-wasm/src/worker.rs
- confidence: cited
  path: website/docs/concepts/environment-and-admission.md
revision: 8
transitions:
- {from: "draft", to: "proposed", at: "2026-11-25T16:39:22Z", actor: "human:operator", revision: 6, decided_on: {"recorded":{"review_outcome":8}}}
- {from: "proposed", to: "active", at: "2026-11-25T16:39:23Z", actor: "human:operator", revision: 7, decided_on: {"recorded":{"review_outcome":8}}}
- {from: "active", to: "implemented", at: "2026-11-25T16:39:23Z", actor: "human:operator", revision: 8, decided_on: {"recorded":{"test_result":1,"review_outcome":8}}}
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

- `crates/spindle-ingress/src/surface.rs`, `src/lib.rs` — `Caller::fixed`, `Caller::repeated` and
  `Unfixed`: `run`'s check of its fixed callers, moved out of `spindle-cli` so every host calls it.
- `crates/spindle-cli/src/serve.rs` — `run` calls them; its messages are unchanged.
- `crates/spindle-embed/src/lib.rs` — `Opening::callers`, `fixed` (the check, each refusal named
  `the fixed actor <actor>...`), the surface made `Surface::fixing_callers`.
- `crates/spindle-wasm/src/worker.rs` — the Service Worker opens with no fixed caller.
- `crates/spindle-node/Cargo.toml`, `src/lib.rs` (`Config::fixed_actors`, `FixedActor`),
  `src/export.rs`.
- `crates/spindle-node/tests/node/fixed-actor.test.mjs`,
  `tests/fixtures/fixed-actor-exchanges.json`, `tests/spindle-node/fixed_actor.rs` (the native
  recording and the opening's refusals), `tests/spindle-node/listener.rs` (the replay loop shared),
  `tests/spindle-node/main.rs`.
- `README.md`, `AGENTS.md`, `CHANGELOG.md`, `website/docs/concepts/environment-and-admission.md`.

Shared with open stories (this story merged first, in PR 86; each of them starts from it):

- `crates/spindle-wasm/src/worker.rs`, `Opening::callers` and `Surface::fixing_callers`: `story:service-worker-host-matches-run`, which turns fixed actors on in the Service Worker and `depends_on` this story.
- `crates/spindle-node/tests/node/*.test.mjs`: `story:node-export-release-artifact` runs them and `depends_on` this story.
- `crates/spindle-ingress/src/surface.rs`: `story:front-ingress`, `story:cross-component-view-refused`.
- `crates/spindle-cli/src/serve.rs`: `story:front-ingress`, `story:otlp-trace-export`, `story:request-span-tree`.
- Documentation: `AGENTS.md` (`story:node-export-release-artifact`), `README.md` (`story:spindle-image-published`), `website/docs/concepts/environment-and-admission.md` (`story:reset-reapplies-seed`).

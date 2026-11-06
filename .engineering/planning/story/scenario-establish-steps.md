---
format: aep.planning-md/3
id: story:scenario-establish-steps
kind: story
status: implemented
title: Scenario replay establishes state from fixtures
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-conformance/src/target.rs
- confidence: inferred
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-kernel
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-05T22:53:34Z", actor: "agent:b10x-chef-orchestrator", revision: 5}
- {from: "proposed", to: "active", at: "2026-11-05T22:53:34Z", actor: "agent:b10x-chef-orchestrator", revision: 6}
- {from: "active", to: "implemented", at: "2026-11-06T03:30:11Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"test_result":2}}}
---
# Scenario replay establishes state from fixtures

## Outcome

A conformance scenario that arranges its starting state with `establish` steps (an entity in a
named state, with its fields) replays on the host: `HostTarget` writes the established row into
the component's store that owns the entity, as ESS's interpreter does, and the scenario's commands
then run against it. Today `HostTarget` answers every establish step unsupported
(`crates/spindle-conformance/src/target.rs:185`, "the in-process host holds only state its
commands wrote"), so a suite whose scenarios seed rows this way executes no command at all.

## Acceptance

- `an_established_row_is_read_by_the_next_command` — a scenario that establishes an entity in a
  non-initial state and then sends a command guarded on that state passes on the host and on ESS's
  interpreter with the same outcome and view rows.
- `an_established_row_is_published_by_its_views` — after establish, every view over the entity
  answers the row the interpreter answers.
- `establishing_an_entity_another_component_owns_is_written_there` — in a two-component
  specification, the row lands in the owning component's store.
- `the_synthesized_suite_executes_its_establishing_scenarios` — over a neutral fixture, ESS's
  synthesized suite runs on the host with no scenario skipped for establish.

## Origin

Measured by the downstream session on a 20-scenario specification: 19 of 20 scenarios stop at
arrange on this refusal, and it is the only blocker to their replay once
`story:binding-refusal-policies` is in. Pre-existing.

## Scope

- `crates/spindle-conformance/src/target.rs` — cited: the refusal and the arrange path.
- `crates/spindle-host/src/host.rs` — inferred: a way to write an established row into a
  component's store.
- `crates/spindle-kernel` — inferred: the store write and the row shape ESS establishes.

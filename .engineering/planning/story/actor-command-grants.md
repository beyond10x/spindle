---
format: aep.planning-md/3
id: story:actor-command-grants
kind: story
status: implemented
title: A command sent as an actor not granted it is refused
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-conformance/src/target.rs
- confidence: inferred
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-kernel/src/execute.rs
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T00:36:38Z", actor: "agent:b10x-chef-orchestrator", revision: 5}
- {from: "proposed", to: "active", at: "2026-11-06T00:36:39Z", actor: "agent:b10x-chef-orchestrator", revision: 6}
- {from: "active", to: "implemented", at: "2026-11-06T03:30:12Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"test_result":2}}}
---
# A command sent as an actor not granted it is refused

## Outcome

A command sent as an actor whose `may:` does not name that command is refused before anything is
written, as ESS 0.86.0 refuses it (`command_grant`). Today spindle runs it:
`crates/spindle-conformance/src/target.rs` states that `HostTarget` "checks no grant", and the host
sends a command as the actor it is given (`Host::submit_as`).

## Acceptance

- `a_command_the_actor_may_not_send_is_refused` — an actor whose `may:` lists only `ReadBook`
  sends `DeleteBook`: refused, no row and no event written.
- `a_command_the_actor_may_send_runs` — the same actor sends `ReadBook` and it runs.
- `command_grants_agree_with_the_interpreter` — the grant scenarios replay with the outcome ESS's
  interpreter gives.

## Origin

Pre-existing; noted during wave 6.

## Scope

- `crates/spindle-conformance/src/target.rs` — cited: the "checks no grant" note on `HostTarget`.
- `crates/spindle-host/src/host.rs` — inferred: `submit_as`, where the actor is known.
- `crates/spindle-kernel/src/execute.rs` — inferred: where the refusal could be decided.

## Evidence

Measured by two consumers on 2026-11-06: 3 of 20 scenarios of a downstream specification, and 5
grant-denied scenarios of a second consumer's specification, run the command on spindle instead of
refusing it "not granted" as ESS's interpreter does.

Acceptance, in addition to the lines above: the refusal's text ("not granted") and its outcome
match ESS's interpreter, so the same scenarios pass on spindle and on the interpreter.

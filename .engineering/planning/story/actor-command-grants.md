---
format: aep.planning-md/3
id: story:actor-command-grants
kind: story
status: draft
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
revision: 3
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

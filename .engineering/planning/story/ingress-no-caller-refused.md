---
format: aep.planning-md/3
id: story:ingress-no-caller-refused
kind: story
status: archived
title: A request with no established caller is refused where the specification grants commands
tags:
- from-finding
relations:
- informed_by: story:ingress-caller-identity-specified
revision: 2
transitions:
- {from: "draft", to: "archived", at: "2026-11-09T22:14:00Z", actor: "human:timo", revision: 2}
---
# A request with no established caller is refused where the specification grants commands

## Finding

Filed from wave 17 unit U8 (`story:ingress-caller-identity-specified`), pre-existing on 98212306.
The implementor read the code; the coordinator did not re-run it.

- With no fixed actor, the ingress sends a command as no actor
  (`crates/spindle-ingress/src/answer.rs:225-231`).
- The kernel treats no actor as the runtime's own authority and skips the grant check
  (`crates/spindle-kernel/src/grant.rs:18-20`), so the command runs and gets its model answer.
- A command that reads caller attributes answers 501 instead
  (`crates/spindle-ingress/src/surface.rs:1088`).
- With `--fixed-actor`, an actor without the grant gets 403 not granted (`answer.rs:263`, `:36-41`).

For a component `reached_by: network`, ESS's synthesised `grant/denied` scenarios expect 403
`{"refused":"not granted","actor":null}` for a request with no caller
(`ess-conformance src/synthesize/grant.rs:3-5`, ESS 0.90.0).

## Outcome, to be decided

Whether a served (network) component's command with no established caller answers ESS's standard
refusal instead of running unchecked. This is a behaviour change in `crates/` and is the operator's
to accept.

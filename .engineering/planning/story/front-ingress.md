---
format: aep.planning-md/3
id: story:front-ingress
kind: story
status: draft
title: One front ingress serves a split run's whole surface
tags:
- needs-timo-acceptance
relations:
- serves: vision:spindle
- decomposes: epic:split-run
- depends_on: story:run-one-component
- depends_on: story:http-caller-proves-actor
scope:
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli
- confidence: cited
  path: crates/spindle-ingress/src/console.rs
- confidence: inferred
  path: crates/spindle-ingress/src/forward.rs
- confidence: cited
  path: crates/spindle-ingress/src/listener.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
revision: 4
---
# One front ingress serves a split run's whole surface

**Needs Timo's acceptance**: a process in front of the components changes how a split run is
reached.

## Outcome

`b10x-spindle run <spec> --environment <file> --front` serves every route of the specification
and the console routes, runs no component, and forwards each command or view to the process of the
component that accepts or owns it, at `SPINDLE_PEER_<COMPONENT>`'s ingress address. The answer is
the component's answer, byte for byte. It proves callers as `story:http-caller-proves-actor`
decides and passes the proven caller on to the component over a channel only the front and the
components share. `/api/events` gathers every component's answer; `POST /api/reset` resets every
component.

## Acceptance

- `the_front_reaches_the_accepting_component`: with rally split, a command the table accepts, sent
  to the front, is answered by the table's process with the same status and body a one-process
  `run` answers.
- `an_unreachable_component_answers_503`: with one component process stopped, a request routed to
  it answers `503` naming the component.
- `other_components_keep_answering`: with that process stopped, a request routed to another
  component is answered as before.
- `the_front_forwards_the_proven_caller`: a command that reads its caller runs for the caller the
  front proved.
- `a_component_refuses_a_caller_sent_around_the_front`: a request carrying a caller, sent straight
  to a component process, is refused.
- `the_front_gathers_events`: `/api/events` at the front lists every component's events.

## Scope

- `crates/spindle-ingress/src/surface.rs`, `crates/spindle-ingress/src/listener.rs` (routing to a
  forwarder instead of an `Exchange`), `crates/spindle-ingress/src/console.rs`.
- `crates/spindle-ingress/src/forward.rs` (new).
- `crates/spindle-cli/src/serve.rs`, `crates/spindle-cli/src/cli.rs` (`--front`).
- `crates/spindle-cli/tests/`.

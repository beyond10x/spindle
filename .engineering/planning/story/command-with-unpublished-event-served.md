---
format: aep.planning-md/3
id: story:command-with-unpublished-event-served
kind: story
status: draft
title: A command whose event no component publishes is served
summary: A command emitting an event no component publishes answers its outcome as ESS's interpreter does; the event stays unpublished, instead of a 501.
relations:
- serves: vision:spindle
revision: 1
---
# A command whose event no component publishes is served

## Outcome

A command whose outcome emits an event that no component lists under `publishes:` is served: the
route answers the command's outcome, the state changes as ESS's interpreter changes it, and the
event stays unpublished. Spindle invents nothing for it: no publication, no channel, no row.

## Why

ESS rules that leaving a command's emitted event out of every component's `publishes:` is valid:
emission and publication are separate, and ESS's generated ports already treat an emitted,
unpublished event as a no-op (ESS draft
`story:a-component-that-publishes-nothing-keeps-the-events-its-commands-emit-unpublished`).
A consumer's component publishes nothing and declares the event of a created identity only because
a `creates:` outcome's `instance:` names a field of an emitted event.

Spindle refuses such a command with "{component} does not publish {event}"
(`crates/spindle-channel/src/channel.rs:82-83`, `ChannelError::NotPublished`), and the route
answers 501.

## Not covered

A create that emits no event at all is refused by ESS (`ESS-COMMAND-001`, no field of an emitted
event names the created identity) and keeps its refusal here.

## Acceptance

- `a_command_with_an_unpublished_event_is_served`: a fixture whose component publishes nothing
  and whose create emits an event; red today with the 501 naming "does not publish". After: the
  command answers the same outcome as ESS's interpreter, the view of the created row answers the
  interpreter's rows, and the host's published events hold no event of it.

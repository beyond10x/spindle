---
format: aep.planning-md/3
id: story:command-with-unpublished-event-served
kind: story
status: implemented
title: A command whose event no component publishes is served
summary: A command emitting an event no component publishes answers its outcome as ESS's interpreter does; the event stays unpublished, instead of a 501.
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-channel/tests/spindle-channel/channel.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/fixtures/unpublished
- confidence: cited
  path: crates/spindle-ingress/tests/spindle-ingress/main.rs
revision: 20
transitions:
- {from: "draft", to: "proposed", at: "2026-11-26T03:50:23Z", actor: "human:operator", revision: 14, decided_on: {"recorded":{"review_outcome":7}}}
- {from: "proposed", to: "active", at: "2026-11-26T03:50:24Z", actor: "human:operator", revision: 15, decided_on: {"recorded":{"review_outcome":7}}}
- {from: "active", to: "implemented", at: "2026-11-26T08:12:31Z", actor: "human:operator", revision: 20, decided_on: {"recorded":{"test_result":1,"review_outcome":8}}}
---
# A command whose event no component publishes is served

## Outcome

A command whose outcome emits an event that no component lists under `publishes:` is served. The
route answers the command's outcome, where it answered 501 after the row and the event were
already written. The event is committed to the component's own store with the write, as today;
`Host::published` and the response's `published` array list it, as ESS 0.99.1's interpreter lists
every emitted event (`interpret/command.rs` `complete_command`). It is not handed to the channel:
`publishes:` decides what leaves a component, not what the command emits. No binding can name
such an event (`Router::build` refuses one at start), so no delivery is dropped.

A command that emits a published event and an unpublished one sends the published event's
envelope in the same command. Today the refusal of the second returns before the first is marked
durable and flushed, so that envelope waits in the outbox for a later command.

The change is in `Host::execute`: an event no component publishes is not handed to
`Sender::publish`, which keeps refusing an event its component does not publish for any caller.

## Why

A consumer's component publishes nothing and declares the event of a created identity only because
a `creates:` outcome's `instance:` names a field of an emitted event (ESS-COMMAND-001). ESS keeps
emission and publication apart: its interpreter records every emitted event whatever `publishes:`
says, and its generated ports treat an emitted, unpublished event as a no-op. The ESS ruling is
drafted in the `ess` repository's planning
(`decision-blocker:a-create-names-its-identity-without-a-published-event`), not yet on its main.

Spindle refuses such a command per command at run time: `Sender::publish` returns
`ChannelError::NotPublished` ("{component} does not publish {event}",
`crates/spindle-channel/src/endpoint.rs`, declared at `crates/spindle-channel/src/channel.rs`),
after the kernel committed the row and the event, and the route answers 501 with
`committed: true`.

## Not covered

A create that emits no event at all is refused by ESS (`ESS-COMMAND-001`, no field of an emitted
event names the created identity) and keeps its refusal here.

- An event its component does not publish but another component does keeps its refusal
  (`Sender::publish` answers `ChannelError::NotPublished`, the route answers 501 with
  `committed: true`) and is not delivered, although ESS 0.99.1's interpreter invokes a binding on
  it (`adversary_unpublished_event::an_event_another_component_publishes_reaches_its_binding`).

## Acceptance

A new fixture `crates/spindle-ingress/tests/fixtures/unpublished/` holds two components reached
by network. `desk` owns the domain `helpdesk.tickets` (its events are `helpdesk.tickets.Opened`, `helpdesk.tickets.Touched` and `helpdesk.tickets.Noted`):

- `desk` owns `Ticket`, publishes only `Touched`, and accepts `OpenTicket` (a `creates:` outcome
  `opened` whose `instance:` names the `ticket_id` of the emitted `Opened`; the binding answers
  `opened` with 201) and `TouchTicket` (outcome `touched`, answered 200, emits `Touched`, then
  `Noted`). `Opened` and `Noted` are in no component's `publishes:`.
- `log` owns the domain `helpdesk.log` and binds `Touched` to its command `RecordMark`, which keeps one `Mark` row per `Touched` received, read through its view
  `Marks`.

The test compiles the fixture with ESS 0.99.1, which refuses a specification that does not
validate.

- `a_command_with_an_unpublished_event_is_served`: `POST` of `OpenTicket` with `ticket_id` `t1`
  answers 201 with the body `{"outcome": "opened", "published": [{"event": "helpdesk.tickets.Opened", "payload":
  {"ticket_id": "t1"}}]}`, where it answered 501 naming "does not publish".
- `an_unpublished_event_is_kept`: after that command, `Host::published` lists one event, `helpdesk.tickets.Opened`
  with `ticket_id` `t1`, as ESS 0.99.1's interpreter does: `Interpreted::for_model` over the same compiled model, driven through `ConformanceTarget::execute_command` with the same command, answers one entry in `SemanticCommandResult::direct_events`.
- `a_published_event_beside_an_unpublished_one_is_sent`: after `OpenTicket` of `t1`, then one `TouchTicket` of `t1` (subject `t1` exists, so `touched` is selected) and no command after it,
  `log`'s view `Marks` answers one row. Today the refusal of `Noted` returns
  before `Touched` is marked durable and flushed, and `Marks` answers none.
- `the_unpublished_event_of_a_touch_is_kept`: after that `TouchTicket`, `Host::published` lists
  `helpdesk.tickets.Touched` then `helpdesk.tickets.Noted` for `t1`, after the `Opened` of `t1`.
- `Sender::publish` keeps refusing an event its component does not publish and stages nothing:
  the existing `a_component_publishes_only_its_own_events`
  (`crates/spindle-channel/tests/spindle-channel/channel.rs`) passes unchanged.

## Scope

- `crates/spindle-host/src/host.rs` (`Host::execute`, the loop that hands each emitted event to
  `held.sender.publish`, the only caller of `Sender::publish` outside tests): an event no
  component publishes is skipped (one another component publishes is handed over and refused
  as before); `held.position`, `mark_durable` and `flush` run when an
  event was staged.
- Unchanged: `crates/spindle-channel/src/endpoint.rs` (`Sender::publish` keeps
  `ChannelError::NotPublished`), `crates/spindle-ingress/src/answer.rs` (the 501 mapping of a
  channel error stays for other channel errors), `Host::published`.
- Fixture: `crates/spindle-ingress/tests/fixtures/unpublished/` (new); tests: a new module in the
  existing `crates/spindle-ingress/tests/spindle-ingress/` binary and its `mod` line in
  `main.rs`.
- Documents: `CHANGELOG.md` `[Unreleased]` (one bullet).

Where the refusal happens today: per command at run time, after the kernel committed the row and
the event (`crates/spindle-kernel/src/execute.rs`), in `Sender::publish`
(`crates/spindle-channel/src/endpoint.rs`), mapped to 501 with `committed: true` in
`crates/spindle-ingress/src/answer.rs`. A running host has no binding on an event no component
publishes: `Router::build` refuses one at start (`crates/spindle-channel/src/route.rs`), so
skipping the event drops no delivery.

## Shared files

- `crates/spindle-host/src/host.rs`: shared at file level with `story:view-paging` (merged in
  PR 88) and the drafts `binding-external-timer-durations`, `cross-component-view-refused`,
  `node-export-clock-and-seeds`, `periodic-bindings-retry-on-failure`, `queue-families`,
  `request-id-idempotency`, `service-worker-host-matches-run`,
  `settle-error-keeps-report`, `several-writers-one-postgres`,
  `store-and-kernel-durations`, `view-read-families`,
  `wasi-timers-driven` and `world-row-integer-fields`. None of them names the publish loop of
  `Host::execute`; the overlap is accepted and whichever lands second rebases.
- `crates/spindle-ingress/tests/spindle-ingress/main.rs`: every story that adds an ingress test
  module adds one line.
- `CHANGELOG.md` `[Unreleased]`: every story that ships adds a bullet.
- `Sender::publish` in `crates/spindle-channel/src/endpoint.rs`: `story:committed-events-survive-restart` restages committed events after a restart, a second caller; it runs after this story (`depends_on`) and skips an event no component publishes, as `Host::execute` does.

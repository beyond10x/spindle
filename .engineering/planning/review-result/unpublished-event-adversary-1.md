---
format: aep.planning-md/3
id: review-result:unpublished-event-adversary-1
kind: review-result
status: active
title: Adversary, round 1, on the unpublished event story
relations:
- reviews: story:command-with-unpublished-event-served
revision: 1
---
needs-revision

```
unit: story:command-with-unpublished-event-served, commit 12b3afed
verdict: NEEDS-CHANGE
cases: executed 114→116 (spindle-ingress binary), red 1
origin: introduced 1 / pre-existing 0 / undecided 0
```

Cases added in `crates/spindle-ingress/tests/spindle-ingress/adversary_unpublished_event.rs` on the fixture `crates/spindle-ingress/tests/fixtures/adversary-unpublished/`:

- `an_event_another_component_publishes_reaches_its_binding`: red on 12b3afed. An event the emitting component does not publish, but another component does, was skipped in silence and the command answered its outcome; ESS 0.99.1's interpreter invokes the binding on it, and the base refused with NotPublished. The test accepts the delivery or the refusal.
- `a_published_event_between_unpublished_ones_is_sent_in_order`: green.

Fixed in 604da57d: `Host::execute` skips an event only when no component publishes it; an event another component publishes is handed to `Sender::publish` and refused as before. The story's Outcome, Scope, Not covered and Shared files say so. F3 is kept as is: `an_unpublished_event_is_kept` checks the acceptance statement, which the base also met, since the kernel committed the event before the refusal.

```findings
- file: crates/spindle-host/src/host.rs
  line: 1775
  category: contract-drift
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the filter skipped an event its own component does not publish although another component publishes it and a binding routes it; the event was dropped with 200 where the base refused it and ESS delivers it
- file: .engineering/planning/story/command-with-unpublished-event-served.md
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the story claimed no delivery is dropped for every event its component does not publish; true only for an event no component publishes
- file: crates/spindle-ingress/tests/spindle-ingress/unpublished_event.rs
  category: acceptance
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: an_unpublished_event_is_kept also passes on the base commit
```

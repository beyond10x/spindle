---
format: aep.planning-md/3
id: story:event-payload-absent-optional-null
kind: story
status: draft
title: An event carries an Optional field the row holds as null, as ESS does
relations:
- serves: vision:spindle
revision: 1
---
# An event carries an Optional field the row holds as null, as ESS does

## Outcome

An event payload field of an `Optional` type whose source is a stored field holding `null` (a
row a scenario established with the field `null`) is carried as `null`, and one whose source
is absent is carried as `null` where the field declares `null_when_absent` and left out
otherwise, as ESS's interpreter emits it (`interpret::execute::emit`, beyond10x/ess#765). Today
Spindle holds an established `null` as absent and leaves the field out, so a scenario stating
`null` fails against the host while it passes against the interpreter.

View rows follow a different ESS rule (a view field a row holds nothing in is `null` unless it
declares `omitted_when_absent`); `story:view-rows-absent-optional-fields` covers them, and this
change does not.

## Acceptance

- `an_event_carries_a_held_null_as_null` — an established row whose `Optional` field is `null`,
  copied into an event by `{subject: …}`: the host carries `null`, equal to ESS's interpreter,
  and the authored scenario stating `null` passes on both.

## Scope

- `crates/spindle-conformance/src/establish.rs` — a top-level `null` is held.
- `crates/spindle-kernel/src/values.rs`, `crates/spindle-kernel/src/execute.rs` — stored-field
  reads keep a held `null`; `null_when_absent` on an event field.

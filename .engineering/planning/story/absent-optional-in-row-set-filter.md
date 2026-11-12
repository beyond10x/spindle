---
format: aep.planning-md/3
id: story:absent-optional-in-row-set-filter
kind: story
status: implemented
title: A row-set guard over an absent Optional field names the field and the way out when it refuses as undecidable
tags:
- downstream-gap
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T22:22:01Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T22:22:02Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-12T01:42:33Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---


## Outcome

A command whose refusal outcome counts related rows with `when_related: {entity, where: [<optional
field> == input.<x>], count: {gte: 1}}` is refused `501 undecidable` as soon as one stored row of
that entity has the optional field absent, even though that row plainly is not in the set the
author means. With no rows, or only rows whose field is set, the same command answers 202 or 409
correctly. The refusal text names only the outcome, not the field or the row that left the guard
open, so the author cannot tell why a "not empty" guard cannot be decided.

Observed on b10x-spindle 0.8.0, ess 0.90.0 (`format: ess/35`).

## Minimal model

`system.yaml`: `format: ess/35`, `system: store`, `domains: [store.shelf]`. The domain file:

```yaml
domain: store.shelf
summary: Boxes, and items that may or may not sit in a box.
naming: {wire: shelf, display: Shelf}

errors:
  - {name: store.shelf.BoxNotFound, summary: No box has this id., fields: []}
  - name: store.shelf.BoxStateConflict
    summary: The box is not Open.
    fields: [{name: state, type: store.shelf.Box.State}]
  - name: store.shelf.BoxNotEmpty
    summary: Items are still in the box.
    fields: [{name: id, type: Uuid}]
  - {name: store.shelf.ItemNotFound, summary: No item has this id., fields: []}

entities:
  - name: store.shelf.Box
    naming: {summary: A box that holds items.}
    identity: {name: id, type: Uuid}
    fields: [{name: label, type: String}]
    lifecycle:
      initial: Open
      states: [Open, Retired]
      terminal: [Retired]
      transitions: [{name: retire, from: [Open], to: Retired}]
  - name: store.shelf.Item
    naming: {summary: An item; it sits in one box or in none.}
    identity: {name: id, type: Uuid}
    fields:
      - {name: label, type: String}
      - {name: box_id, type: Optional<Uuid>}
    relations:
      - {name: box, kind: references, target: store.shelf.Box, cardinality: one, via: box_id}
    lifecycle: {initial: Stored, states: [Stored], terminal: [Stored]}

commands:
  - name: store.shelf.RetireBox
    naming: {display: Retire a box, summary: Only a box with no item in it is retired.}
    input: [{name: id, type: Uuid}]
    outcomes:
      - {name: unknown, unknown_instance: true, error: store.shelf.BoxNotFound}
      - name: not-empty
        when_related:
          entity: store.shelf.Item
          where: [box_id == input.id]
          count: {gte: 1}
        error: store.shelf.BoxNotEmpty
        payload: {store.shelf.BoxNotEmpty: {id: input.id}}
      - name: retired
        moves: store.shelf.Box.retire
        instance: id
        emits: [store.shelf.BoxRetired]
        payload: {store.shelf.BoxRetired: {id: input.id}}
      - name: wrong-state
        wrong_state: true
        error: store.shelf.BoxStateConflict
        payload: {store.shelf.BoxStateConflict: {state: {subject: state}}}
  # CreateBox(label), AddItem(label) -> Item with box_id absent,
  # PutItemInBox(id, box_id) -> sets box_id; the events and the Keeper actor are the obvious ones.
```

The full, validating model (`ess specify validate`: "store v1 — 3 file(s), valid"; `components.yaml`
accepts the four commands) is `model/` beside this draft; it is 4 small files.

## Transcript

`b10x-spindle run model --environment examples/environments/laptop.toml --state <dir> --observe`,
then `POST /shelf/commands/<Command>`:

```
### (a) no items: RetireBox
POST CreateBox {"label":"a"}            HTTP 202 outcome created
POST RetireBox {"id":"<a>"}             HTTP 202 {"outcome":"retired",...}
### (b) one item in another box: RetireBox
POST CreateBox b / CreateBox other / AddItem i1 / PutItemInBox {i1 -> other}   HTTP 202 each
POST RetireBox {"id":"<b>"}             HTTP 202 {"outcome":"retired",...}
### (c) one item with no box: RetireBox
POST CreateBox c                        HTTP 202
POST AddItem {"label":"i2"}             HTTP 202   (box_id absent)
POST RetireBox {"id":"<c>"}             HTTP 501 {"committed":false,"refused":"the guard of `store.shelf.RetireBox/not-empty` is undecidable over the held instance and the input"}
```

Case (c) needs only the one ungrouped item; box `c` is empty and the item sits in no box at all.
Full transcript with ids: `transcript.txt`.

## Cause

Confirmed, and it is the documented three-valued reading, not a stray bug:

- `crates/spindle-kernel/src/facts.rs:12` and `:140` (`Value::Null => {}`): a null binds nothing,
  and the module doc says "a guard over an absent value is `Unknown`, never `false`".
- `crates/spindle-kernel/src/execute.rs:777` (`row_set`) evaluates the `where` filter on every stored
  `Item`. For the row with `box_id` absent, `box_id == input.id` does not resolve both operands, so
  ESS's `Predicate::evaluate_compare` answers `Unknown`
  (`ess/crates/specify/ess-primitives/src/predicate.rs:2231`, the `else { return (Truth::Unknown, None) }`).
  That row is a possible member of the set.
- `execute.rs:787` calls `ResolvedRowSetTest::decide`
  (`ess/crates/specify/ess-compiler/src/ir.rs:1702`): `count >= 1` holds for 1 possible member and
  fails for 0, so `certain = 0, possible = 1` is not the same for every completion: `Unknown`.
- `execute.rs:589` (`holds`) turns `Truth::Unknown` into `ExecuteError::Undecidable`
  (`crates/spindle-kernel/src/lib.rs:154-157`), mapped to HTTP 501 by the host.

Spindle follows ESS here: the interpreter reads the same selector and uses the same `decide`
(`ess/crates/verify/ess-conformance/src/interpret/execute/row_set.rs:220`).

## Expected behaviour

The first expectation, "an absent optional compared with `==` to a value is false", is not what
ESS specifies. ESS says the opposite: `docs/design/cross-record-and-stored-field-guards.md:180`
("Optional fields. May be read. `evaluate_compare` answers `Unknown` when an operand does not
resolve ... An author who wants `None` to select something writes `not defined(field)`"). So this
story does not ask spindle to diverge from ESS.

What spindle can improve on its own:

1. The refusal names the guard's field and the open row: for example "the guard of
   `store.shelf.RetireBox/not-empty` is undecidable: `box_id == input.id` is unknown on 1
   `store.shelf.Item` row where `box_id` is absent (write `defined(box_id)` in the filter)".
2. The workaround is documented where `when_related` is documented in spindle's own docs.

What is a question for ESS (to be raised there, not decided here): whether a row-set `where` over
an Optional field should select no row when the field is absent, since the author of `[box_id ==
input.id]` means "rows in this box". Until then the workaround, verified on 0.8.0, is to write the
guard so it is false for an absent value:

```yaml
where: [defined(box_id), box_id == input.id]
```

With that single change cases (a), (b) and (c) all answer 202 (`transcript-workaround.txt`).

## Acceptance

- `a_row_set_guard_over_an_absent_optional_names_the_field` - the model above, one `Item` with
  `box_id` absent, `RetireBox` answers 501 `undecidable` and the message names `box_id` and the
  entity `store.shelf.Item`.
- `a_row_set_guard_with_defined_over_an_absent_optional_is_decided` - same model with `where:
  [defined(box_id), box_id == input.id]`: (a), (b) and (c) answer 202.
- `a_row_set_guard_over_an_absent_optional_agrees_with_the_interpreter` - case (c) replays with the
  outcome ESS's interpreter gives (refusal, unchanged by this story).

## Origin

Found while running a private specification on 0.8.0: a "retire a group only when empty" command
refused every time one member row had the grouping field absent. Reduced to the neutral model above.

## Scope

- `crates/spindle-kernel/src/execute.rs` - cited: `row_set` (777, 787) and the `Undecidable` at 589.
- `crates/spindle-kernel/src/lib.rs` - cited: `ExecuteError::Undecidable` (154-157), where the
  message could carry the guard and field.
- `crates/spindle-kernel/src/facts.rs` - cited: the null-binds-nothing rule (12, 140).

## Evidence

`transcript.txt` (cases a, b, c), `transcript-workaround.txt` (same with `defined(box_id)`), model
in `model/`, validated with ess 0.90.0, run on b10x-spindle 0.8.0 `x86_64-unknown-linux-gnu`.

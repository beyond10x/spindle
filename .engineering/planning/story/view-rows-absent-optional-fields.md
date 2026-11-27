---
format: aep.planning-md/3
id: story:view-rows-absent-optional-fields
kind: story
status: active
title: A view row shows an optional field never written as ESS does
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-conformance/src/target.rs
- confidence: inferred
  path: crates/spindle-conformance/tests/fixtures/absent-view-fields
- confidence: inferred
  path: crates/spindle-conformance/tests/spindle-conformance/absent_view_fields.rs
- confidence: inferred
  path: crates/spindle-conformance/tests/spindle-conformance/main.rs
- confidence: cited
  path: crates/spindle-host/src/view.rs
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/views.rs
revision: 13
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T09:51:58Z", actor: "human:operator", revision: 12, decided_on: {"recorded":{"review_outcome":8}}}
- {from: "proposed", to: "active", at: "2026-11-27T09:51:58Z", actor: "human:operator", revision: 13, decided_on: {"recorded":{"review_outcome":8}}}
---
# A view row shows an optional field never written as ESS does

## Outcome

A view row read through the conformance target (`HostTarget::query_view`) whose source row never
wrote an `Optional` field answers that field as ESS 0.99.1's interpreter does: `null` unless the
field's presence is `omitted_when_absent`. Today spindle leaves the field out unless its presence is
`null_when_absent` (`crates/spindle-host/src/view.rs`, `project`), so replayed scenarios disagree
with ESS on the same state. Served views keep ESS's published contract, which declares such a field
optional and not nullable (decision D1 A).

## Acceptance

- `an_optional_field_never_written_is_answered_as_ess_answers_it`: an entity created without its
  optional field, read through `HostTarget::query_view` with a view that projects it: the row
  equals ESS's interpreter's row for the same steps, key present or absent as ESS has it.
- `view_rows_agree_with_the_interpreter_on_absent_fields`: for a scenario with written and
  unwritten optional fields, every row `HostTarget::query_view` answers equals the interpreter's.
- `a_served_view_row_still_leaves_an_unwritten_optional_field_out`: the same entity read through a
  served view (`Host::listed_with`) answers its row without the field.

## Origin

Pre-existing. Found by the `story:kernel-caller-attributes` unit (wave 6) while comparing view rows
with the interpreter.

## Scope

Derived 2026-11-27 by `story-scoper` at 394adc71, against ESS 0.99.1 (rev d7833ee; 0.99.2's
interpreter is unchanged). Every line is **cited** (read from the story or the tree) or **inferred**
(a reading that could be wrong).

- **Primary surface:** under D1=A (default) `crates/spindle-conformance`, the conformance target;
  under D1=B `crates/spindle-host`, view projection — cited, both read in the tree
- **Files:** `crates/spindle-conformance/src/target.rs:461` (`HostTarget::query_view`; the
  undetermined-field check at `:236` already reads the view's declared fields from `self.ir`) — cited
- **Files:** `crates/spindle-host/src/view.rs:1250-1329` (`project`: rule at `1313-1326`, stored
  `null` dropped at `1315`, reads through a reference at `1293-1307`, doc comment at `1225-1235`);
  under A only the doc comment may change — cited
- **Symbols:** `HostTarget::query_view`, `view::project`, `Presence::NullWhenAbsent`,
  `Presence::OmittedWhenAbsent` — cited
- **Parity rule (ESS 0.99.1 `crates/verify/ess-conformance/src/interpret/views.rs:449-461`):** an
  `Optional` field the row does not hold is `null` unless the field declares `omitted_when_absent`;
  a read through a reference that names no row is `null` (`:228-236`); a stored `null` is kept —
  cited
- **Contract conflict:** ESS's published contract for a field with no policy is optional and not
  nullable (`crates/generate/ess-gen/src/types.rs:870-890`, `is_nullable` at `:789`); spindle serves
  that contract (`spindle-cli/src/serve.rs:58`, `spindle-embed/src/lib.rs:231`) — cited
- **Tests (new):** a module beside
  `crates/spindle-conformance/tests/spindle-conformance/held_null.rs`, registered in `main.rs`, with
  a fixture under `crates/spindle-conformance/tests/fixtures/` — inferred
- **Also likely, under B only:** the helpers in `crates/spindle-host/tests/spindle-host/` that drop
  the interpreter's `null`s before comparing (`related.rs:189`, `filter_through.rs:232`,
  `text_filters.rs:182`, `parameter_literal_filters.rs:227`, `always_and_key_filters.rs:180`,
  `computed.rs:300/331/342`, `adversary_w26_u2.rs:193`) fail and must compare exactly; the
  acceptance tests then go in `crates/spindle-host/tests/spindle-host/views.rs` — inferred
- **Also likely, optional cleanup under A:** `spindle-conformance` tests that drop `null` on both
  sides (`explore.rs:434`, `grants.rs:107`, `adversary_caller.rs:139`,
  `adversary_establish.rs:160`) keep passing and could compare exactly — inferred
- **Documents:** none — inferred
- **Confidence:** medium — the defect site and ESS's rule are read in the tree; which surface lands
  depends on D1
- **Would collide with:** any unit changing `spindle-conformance`'s view query adapter
  (`target.rs`); any unit appending a module to
  `crates/spindle-conformance/tests/spindle-conformance/main.rs`; under B, any unit in
  `spindle-host`'s `view.rs` (filter, order, references) or its view-comparison test helpers
- **Safety fact:** under A, only the conformance runner calls `HostTarget::query_view`; served view
  routes read `Host::listed_with` directly, so served bodies do not change — level 2 (`target.rs:461`
  calls `running.host.listed_with`), unproven

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: only the conformance target (`HostTarget::query_view`) answers an `Optional` field
  never written as ESS's interpreter does (`null` unless `omitted_when_absent`, `null` through a
  reference naming no row, a stored `null` kept); served views keep ESS's published contract,
  which declares such a field optional and not nullable.

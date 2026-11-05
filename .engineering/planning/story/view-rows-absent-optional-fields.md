---
format: aep.planning-md/3
id: story:view-rows-absent-optional-fields
kind: story
status: draft
title: A view row shows an optional field never written as ESS does
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-host/src/view.rs
- confidence: inferred
  path: crates/spindle-host/tests/views.rs
revision: 3
---
# A view row shows an optional field never written as ESS does

## Outcome

A view row whose source row never wrote an `Optional` field answers that field as ESS 0.86.0's
interpreter does: ESS's view rows carry it as `null`, spindle's leave it out unless the field's
presence is `null_when_absent` (`crates/spindle-host/src/view.rs`, `project`). Replayed scenarios
and served views then disagree with ESS on the same state.

## Acceptance

- `an_optional_field_never_written_is_answered_as_ess_answers_it` — an entity created without its
  optional field, read through a view that projects it: the row equals ESS's interpreter's row for
  the same steps, key present or absent as ESS has it.
- `view_rows_agree_with_the_interpreter_on_absent_fields` — for a scenario with written and
  unwritten optional fields, every view row equals the interpreter's.

## Origin

Pre-existing. Found by the `story:kernel-caller-attributes` unit (wave 6) while comparing view rows
with the interpreter.

## Scope

- `crates/spindle-host/src/view.rs` — cited: `project`, the presence rule.
- `crates/spindle-host/tests/views.rs` — inferred: the tests.

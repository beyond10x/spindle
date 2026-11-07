---
format: aep.planning-md/3
id: story:kernel-creation-takes-unknown-row
kind: story
status: draft
title: A creation takes an absent row beside unknown_instance as ESS does
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/creation_takes_unknown_row.rs
revision: 2
---
# A creation takes an absent row beside `unknown_instance:` as ESS does

## Outcome

A command with no row set that declares `unknown_instance:` and has a creation branch taking its
identity from the same input field answers an absent addressed row as ESS's interpreter does
(`addressed_unknown`): the creation may take the row, and the `unknown_instance:` answer applies
only where no creation is selected; where ESS falls back to its `declared_not_found` answer, the
kernel answers the same. Today the kernel answers every absent addressed row at its existence
step before any creation is considered, outside row sets
(`crates/spindle-kernel/src/execute.rs`, the existence step and `look_ahead`).

## Acceptance

A neutral fixture with such a command, in the spindle-kernel test binary:

- `a_creation_takes_an_absent_row_beside_unknown_instance`: a request for an absent identity whose
  creation guard holds creates the row; one whose guard fails answers the `unknown_instance:`
  branch.
- `addressed_unknown_agrees_with_the_interpreter`: outcomes, errors, payloads and rows over the
  same steps equal ESS's interpreter.

## Scope

- `crates/spindle-kernel/src/execute.rs` — cited: the existence step, `look_ahead`, `take`.
- `crates/spindle-kernel/tests/spindle-kernel/` — inferred: a new module and fixture.

## Origin

Pre-existing; found while fixing the row-set upsert precedence in `story:kernel-row-set-guards`
(wave 14), which made the same rule hold beside a row set only.

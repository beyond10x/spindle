---
format: aep.planning-md/3
id: story:kernel-related-row-values
kind: story
status: draft
title: Values read from a related row
relations:
- serves: vision:spindle
- decomposes: epic:downstream-kernel-constructs
- depends_on: story:kernel-related-row-guards
scope:
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: cited
  path: crates/spindle-kernel/src/values.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/related_values.rs
revision: 4
---
# Values read from a related row

## Outcome

The kernel evaluates a `sets:` or `payload:` value read from a related row (`related_field`,
`related_state`): the field or the held state of the row an input field names, as the store held
it before the outcome. Today `crates/spindle-kernel/src/supported.rs` (`value`) refuses it as "the
value source … of the row input.<field> names".

Requested by a downstream specification probe (2026-11-05, 44 commands): 2 commands are refused
first on this construct. Reproduced with a neutral fixture.

ESS semantics: ESS 0.86.0's interpreter, `crates/verify/ess-conformance/src/interpret/execute.rs`
(`RelatedField`/`RelatedState` read through `reads.related`) and `execute/related.rs`.

## Acceptance

Fixture: the warehouse of `story:kernel-related-row-guards`; a successful `PickItem` creates a
pick that stores the shelf's `aisle` and publishes it.

- `a_created_row_copies_a_field_of_the_related_row` — the pick's `aisle` equals the shelf's.
- `the_related_row_is_read_before_the_outcome` — an outcome that also changes the shelf stores the
  value it held before.
- `a_related_state_value_reads_the_held_state`.
- `related_values_agree_with_the_interpreter` — for the same steps, stored rows and published
  payloads equal what ESS 0.86.0's interpreter produces.

## Scope

- `crates/spindle-kernel/src/supported.rs` — cited: the `other =>` arm of `value`.
- `crates/spindle-kernel/src/values.rs` — cited: `Sources::value`.
- `crates/spindle-kernel/tests/related_values.rs` — inferred, new.

## Depends on

- `story:kernel-related-row-guards` — the same read of the row an input names.

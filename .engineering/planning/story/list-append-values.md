---
format: aep.planning-md/3
id: story:list-append-values
kind: story
status: active
title: List-append values
relations:
- serves: vision:spindle
- decomposes: epic:m0-run-in-process
scope:
- confidence: inferred
  path: crates/spindle-kernel/src/supported.rs
- confidence: cited
  path: crates/spindle-kernel/src/values.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/list_append.rs
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T21:50:12Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T21:50:12Z", actor: "human:timo", revision: 3}
---
# List-append values

## Outcome

The kernel evaluates the list-append value source in `sets:`: the stored list's previous value,
absent read as empty, with the named input value appended last. Today it is refused as an
unsupported value source.

Requested by a downstream specification probe (2026-11-04): one command is refused on this
construct. Reproduced with a neutral fixture.

## Acceptance

- `appending_to_an_absent_list_stores_one_element`.
- `appending_keeps_the_previous_elements_in_order` — the new element is last.
- `a_view_lists_the_appended_elements` — a view row reads the whole list.

## Scope

- `crates/spindle-kernel/src/values.rs` — cited: the value-source refusal.
- `crates/spindle-kernel/src/supported.rs` — inferred: the support check, shared with
  `story:generated-field-values`.
- `crates/spindle-kernel/tests/list_append.rs` — inferred, new.

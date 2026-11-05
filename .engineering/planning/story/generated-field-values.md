---
format: aep.planning-md/3
id: story:generated-field-values
kind: story
status: implemented
title: Generated field values
relations:
- serves: vision:spindle
- decomposes: epic:m0-run-in-process
scope:
- confidence: inferred
  path: crates/spindle-kernel/src/mint.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: inferred
  path: crates/spindle-kernel/src/values.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/generated.rs
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T21:50:10Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T21:50:11Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-05T07:49:23Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":2}}}
---
# Generated field values

## Outcome

The kernel runs an outcome that marks a field `{generated: true}` (and `{generated: true,
present: true}`, and `{input: f, else: {generated: true}}`) on a field other than the minted
creation identity, in `sets:`, an event payload or an error payload. The runtime mints the value
from the field's declared type (`Uuid`, `String` and its newtypes, `Integer`, `Timestamp`), and a
`{payload: <Event>.<field>}` in `sets:` stores the value the same branch published. Today
`crates/spindle-kernel/src/supported.rs` refuses every such field as unsupported.

Requested by a downstream specification probe (2026-11-04): 14 of its commands are refused on
this construct alone. Reproduced here with a neutral fixture, never that specification.

## Acceptance

- `a_generated_stored_field_is_minted_and_kept` — a command whose outcome sets a stored field
  `{generated: true}` stores a value of the declared type, and a view reads the same value back.
- `a_generated_payload_field_matches_the_stored_one` — `{payload: E.f}` in `sets:` stores the value
  the emitted event carries.
- `an_input_value_wins_over_the_generated_else` — `{input: f, else: {generated: true}}` stores the
  input when sent and a minted value when absent.
- `a_generated_value_of_an_unmintable_type_is_refused_by_name` — a type the runtime cannot mint is
  still `unsupported`, naming the field and the type.

## Scope

- `crates/spindle-kernel/src/supported.rs` — cited: the refusal at the `Generated` match.
- `crates/spindle-kernel/src/values.rs` — inferred: value evaluation.
- `crates/spindle-kernel/src/mint.rs` — inferred: minting per type, shared with
  `story:mint-string-newtype-identities`.
- `crates/spindle-kernel/tests/generated.rs` — inferred, new; fixture under `tests/fixtures/`.

ESS semantics: `docs/design/authored-generated-values.md` at ess 0.86.0.

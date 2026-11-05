---
format: aep.planning-md/3
id: story:binding-mappings-across-conversions
kind: story
status: implemented
title: Binding mappings across a declared conversion
relations:
- serves: vision:spindle
- decomposes: epic:downstream-kernel-constructs
scope:
- confidence: cited
  path: crates/spindle-channel/src/route.rs
- confidence: cited
  path: crates/spindle-channel/tests/channel.rs
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-05T12:00:35Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-05T12:00:35Z", actor: "human:timo", revision: 5}
- {from: "active", to: "implemented", at: "2026-11-05T22:03:27Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":2}}}
---
# Binding mappings across a declared conversion

## Outcome

The channel routes an event binding whose mapping carries an event field into a command input of
another declared type across a declared `conversions:` entry: the value is carried unchanged and
checked against the input's declared type before the command runs, and a value that does not
satisfy it is the binding's unmet input, not a crash. Today `crates/spindle-channel/src/route.rs`
refuses such a mapping ("the declared conversion into …"), so `run --scenario` refuses the whole
specification before its first step.

Requested by a downstream specification probe (2026-11-05, 44 commands, 16 bindings): 12 of the
16 bindings map across a conversion, and every one of its 20 scenarios is refused at startup on
the first of them. Reproduced with a neutral fixture.

ESS semantics: ESS 0.86.0's interpreter, `crates/verify/ess-conformance/src/interpret/bindings.rs`
(`mapped`: the value as published, then `validate_typed_value` against the target type).

## Acceptance

Fixture: a library with a lending domain publishing `LoanClosed {member_id: MemberId}` and a
notices domain whose `SendThanks` takes `reader_id: ReaderRef`, a conversion
`MemberId -> ReaderRef` declared, and a binding mapping `reader_id: event.member_id`.

- `a_mapping_across_a_conversion_carries_the_value` — closing a loan runs `SendThanks` with the
  member's identity as `reader_id`.
- `a_value_the_target_type_refuses_is_an_unmet_input` — a value outside the target newtype's
  invariants runs no command and is reported.
- `replay_starts_on_a_specification_with_converted_mappings`.
- `converted_mappings_agree_with_the_interpreter` — for the same scenario, invoked commands and
  resulting rows equal what ESS 0.86.0's interpreter produces.

## Scope

- `crates/spindle-channel/src/route.rs` — cited: the conversion refusal in the mapping rules.
- `crates/spindle-channel/tests/channel.rs` — cited.

## Depends on

Nothing.

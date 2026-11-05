---
format: aep.planning-md/3
id: story:integer-mint-on-ess-ticks
kind: story
status: draft
title: A generated Integer on an ESS tick UUID is minted as ESS mints it
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-kernel/src/mint.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/generated.rs
revision: 3
---
# A generated Integer on an ESS tick UUID is minted as ESS mints it

## Outcome

`{generated: true}` on an `Integer` field, minted from a UUID in ESS's tick form
(`00000000-0000-4000-8000-<tick, 12 decimal digits>`), equals the value ESS 0.86.0's interpreter
mints on that tick, `1 + tick`. Today `crates/spindle-kernel/src/mint.rs` (`value`,
`Minted::Integer`) takes the UUID's low bits, so the decimal digits are read as hex: tick 10 gives
16, not 11.

## Acceptance

- `a_generated_integer_on_a_tick_uuid_is_one_plus_the_tick` — under a mint that answers ESS's tick
  UUIDs, ticks 1, 9 and 10 mint 2, 10 and 11.
- `generated_integers_agree_with_the_interpreter` — for the same command sequence, the minted
  integers equal ESS 0.86.0's interpreter.
- A UUID not in tick form is minted as today (stated in the test).

## Origin

Pre-existing. Found while implementing `story:generated-bytes-and-list-values` (wave 6), whose
`Bytes` mint reads the tick's decimal digits (`byte`) while the `Integer` mint does not.

## Scope

- `crates/spindle-kernel/src/mint.rs` — cited: `value`, the `Minted::Integer` arm.
- `crates/spindle-kernel/tests/generated.rs` — inferred: the tests.

---
format: aep.planning-md/3
id: story:decimal-identity-any-spelling
kind: story
status: draft
title: A command names a Decimal identity whatever its spelling
relations:
- serves: vision:spindle
- informed_by: story:integer-identity-any-spelling
revision: 1
---
## Outcome

A command names a `Decimal` identity whatever its spelling, as ESS 0.99.1's interpreter does: a row
created with `1.0` is found by a command with `1`, and the other way round. Today the kernel keys a
row by the identity's JSON text and rewrites only `Integer` values
(`crates/spindle-kernel/src/typed.rs:53`), so the two spellings are two rows in Spindle and one in
ESS. A `Decimal` value keeps its spelling in rows and events; only the key changes.

Found by the wave 27 U5 adversary; red on the base c61a59650, so pre-existing.

## Acceptance

- `a_decimal_identity_names_one_row_whatever_its_spelling` (`adversary_w27_u5`): its `ignore` is
  removed and it passes; the kernel answers as the interpreter at every step.
- A `Decimal` field beside the identity keeps `2.0` as written in the row and the event payload.

## Not covered

- `Integer` identities: `story:integer-identity-any-spelling`.

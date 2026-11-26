---
format: aep.planning-md/3
id: review-result:wave25-adversary-U6-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 25 U6
relations:
- reviews: story:world-row-integer-fields
revision: 1
---
needs-revision

```
unit: story:world-row-integer-fields, head c8228de4a
verdict: NEEDS-CHANGE
cases: executed 150→156, red 4
origin: introduced 2 / pre-existing 2 / undecided 0
```

Cases in `crates/spindle-host/tests/spindle-host/adversary_world_integers.rs`, red on c8228de4a:

- `a_world_refuses_one_integer_identity_written_twice`: a world holding Shelf `1` and Shelf `1.0`
  loads two rows; the base refused the second as already a row of this world.
- `a_world_reference_names_its_row_whatever_the_integer_spelling`: a reference spelled `1` to an
  identity spelled `1.0` no longer resolves; the base resolved it.
- `a_command_names_a_world_row_whatever_the_integer_spelling`: a world row `1.0` is not found by a
  command naming `1` (base the same).
- `a_world_row_answers_an_integer_written_1_0_as_an_integer`: an Integer field written `1.0` still
  answers `1.0` (base the same).

Attacked and not broken: world row against command row of the same spelling for Decimal, Integer,
2^53+1, `Optional<Integer>` null, `List<Integer>`, struct member and Timestamp; view filter and
`order_by` over mixed spellings; spindle-embed and `run` read through `read_world` alone.

Decided by the coordinator: `seed` writes every value declared `Integer` as a JSON integer, through
`Optional`, newtypes, `List`, `Map` and record members; command input stays as it is.

```findings
- file: crates/spindle-host/src/world.rs
  line: 94
  category: boundary
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "a world with Integer identities 1 and 1.0 loads two rows for one instance"
- file: crates/spindle-host/src/world.rs
  line: 108
  category: boundary
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "a reference spelling one integer as 1 and its target as 1.0 no longer resolves"
- file: crates/spindle-host/src/world.rs
  line: 94
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: "a world row with Integer identity 1.0 is not found by a command naming 1"
- file: CHANGELOG.md
  line: 29
  category: acceptance
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: "an Integer field a world writes as 1.0 still answers 1.0"
```

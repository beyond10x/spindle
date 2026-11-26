---
format: aep.planning-md/3
id: review-result:wave25-adversary-U2-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 25 U2
relations:
- reviews: story:decisions-rules-on-input-predicates
revision: 1
---
needs-revision

```
unit: story:decisions-rules-on-input-predicates, head a78cb87e1
verdict: NEEDS-CHANGE
cases: executed 566→570, red 4 (plus 3 that fail on the base without ui/dist)
origin: introduced 4 / pre-existing 0 / undecided 0
```

Red on a78cb87e1:

- `decisions::adversary_a_when_entry_holding_an_empty_table_is_refused_at_load`: `when = {
  "input.amount" = {} }` loads and the rule matches every request.
- `adversary_served_decisions::adversary_a_reset_world_rule_entry_holding_an_empty_object_is_refused`:
  the same through a reset world's JSON; the reset answers 200.
- `decisions::adversary_a_when_holding_one_path_twice_is_refused_at_load`: one path as a dotted key
  and as a nested table loads as two entries and the rule never matches.
- `decisions::adversary_an_integral_float_request_matches_only_its_own_integer`: `9007199254740992.0`
  matches a rule holding `9007199254740993`.

Attacked and not broken: unknown rule keys and non-`input.` paths refused; `null` in a world rule
refused; non-external outcomes, undeclared commands and a missing answer refused; a refused reset
keeps the previous decider; precedence after a reset; `--answer-default` for another outcome.

Decided by the coordinator: refuse an empty table and a path written twice at load; compare an
integral float with an integer rule value exactly as an integer; test the `13.0`/`13` and nested
`when` deviations.

```findings
- file: crates/spindle-host/src/decisions.rs
  line: 423
  category: boundary
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "a when entry holding an empty table escapes the load-time type check and matches every request"
- file: crates/spindle-host/src/decisions.rs
  line: 415
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "one input path written as a dotted key and as a nested table loads as two conflicting entries"
- file: crates/spindle-host/src/decisions.rs
  line: 92
  category: property
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: "equal compares a float with an integer rule value as f64; integral floats beyond 2^53 match another integer"
- file: crates/spindle-host/src/decisions.rs
  line: 88
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "no test sends an integral float to a rule or writes a nested when table"
```

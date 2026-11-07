---
format: aep.planning-md/3
id: review-result:w12-u2-adversary-1
kind: review-result
status: active
title: Adversary pass 1 on view-list-query-parameters (wave 12)
relations:
- reviews: story:view-list-query-parameters
revision: 1
---
Adversary pass 1 on `story:view-list-query-parameters`, wave 12 unit u2, change ba7aa2b; the
adversary's green cases are committed as 466d930 (module `adversary_view_list_parameter`, fixture
`crates/spindle-cli/tests/fixtures/adversary-view-list-parameter/`). Recorded from the adversary's
report; absolute scratch paths are left out (the repository is public).

```
verdict: NEEDS-CHANGE
cases: executed 8→25, red 2
origin: introduced 1 / pre-existing 0 / undecided 2
```

Red, uncommitted at report time: `a_decimal_element_answers_the_interpreters_rows`
(`adversary_view_list_parameter.rs:357`) and the control `a_scalar_decimal_answers_the_interpreters_rows`
(`:370`):

```
yard.stack.ByAmounts?amounts=3.25&amounts=2 answers the interpreter's rows for {"amounts":[3.25,2]}
  left: (400, {"refused": "yard.stack.ByAmounts: `amounts`: [0]: \"3.25\" is not a Decimal"})
 right: (200, {"rows": [{"crate_id": "C-2"}, {"crate_id": "C-3"}]})
yard.stack.WithAmount?amount=3.25 answers the interpreter's rows for {"amount":3.25}
  left: (400, {"refused": "yard.stack.WithAmount: `amount`: \"3.25\" is not a Decimal"})
 right: (200, {"rows": [{"crate_id": "C-3"}]})
test result: FAILED. 15 passed; 2 failed; 0 ignored; 0 measured; 197 filtered out
```

Attacked and not broken: encoded, bracketed and recased keys; `+`, `%2B`, empty pairs; a repeated
scalar key; Boolean, enum, prefixed identity, Integer and Timestamp elements; 3001 elements;
duplicates; `forall` over a given list and over an absent Optional list; `tags=` on an Optional
list; shadowing and nested binders; a struct element still answering 501; the 400 body shape.

```findings
[
  {"file": "crates/spindle-ingress/src/surface.rs", "line": 186, "category": "boundary", "severity": "warning", "verdict": "NEEDS-CHANGE", "origin": "introduced", "message": "A List<Decimal> element is read as a JSON string, which the host's Decimal check refuses, so every valid request answers 400 where the view answered 501 before and the interpreter answers rows."},
  {"file": "crates/spindle-ingress/src/surface.rs", "line": 204, "category": "boundary", "severity": "warning", "verdict": "CONFIRMED", "origin": "undecided", "message": "A scalar Decimal view parameter answers 400 for a value ESS's published decimal-string schema admits, while the interpreter answers its rows."},
  {"file": "crates/spindle-kernel/src/typed.rs", "line": 215, "category": "contract-drift", "severity": "note", "verdict": "CONFIRMED", "origin": "undecided", "message": "A command input Decimal in the decimal-string spelling ESS publishes is refused as not a Decimal."}
]
```

Coordinator's routing: finding 1 back to the implementor. Finding 2 is pre-existing (the scalar
path is unchanged by the unit) but its fix is the same read the unit owns, so the unit takes it.
Finding 3 is pre-existing, outside the unit's files: filed as draft `story:command-input-decimal-text`.

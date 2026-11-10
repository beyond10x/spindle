---
format: aep.planning-md/3
id: review-result:w19-u3-adversary-1
kind: review-result
status: active
title: Wave 19 U3 adversary pass 1
relations:
- reviews: story:binding-where-condition
revision: 1
---
approve

```
unit: wave 19 U3 story:binding-where-condition, commit f291be6c on 43daf083 (uncommitted test files only)
verdict: CONFIRMED (2 notes)
cases: executed workspace 999→1001, red 0
origin: introduced 2 / pre-existing 0 / undecided 0
```

Cases added in `crates/spindle-cli/tests/spindle-cli/adversary_binding_where.rs`, both green: `every_condition_shape_decides_as_ess_interprets_it` (eight condition shapes over six payloads, host against ESS 0.90.0's interpreter and against hand-worked three-valued results) and `a_null_optional_decides_as_ess_interprets_it`.

Suite: `cargo test --workspace`: 998 passed, 3 failed (known ui/dist), 39 ignored.

Findings:

- `crates/spindle-cli/tests/spindle-cli/binding_where.rs:211`: the `conform` test accepts `unsupported` for `ESS-CF-INVOCATION`, the only condition-dependent check, so it stays green if the condition is ignored; the host-against-interpreter test beside it covers the case. CONFIRMED, introduced, note.
- `docs/design/spindle.md:406`: the unmet-binding-input row lists a missing required field and a failed conversion, not the Unknown condition now reported the same way; the `UnmetInput` comment at `crates/spindle-ingress/src/observe.rs:250` reads likewise. CONFIRMED, introduced, note.

Attacked and not broken: agreement with ESS on every probe scenario; payload conversion; a false condition stages nothing and counts nothing; an Unknown condition gets zero attempts while siblings run and the observe log records `UnmetInput`; an undeclared field is refused by ESS at compile; every publish path reaches `Sender::publish`; retries reuse the staged envelope; `dropping_retries` keeps the condition.

```findings
- file: crates/spindle-cli/tests/spindle-cli/binding_where.rs
  line: 211
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the only condition-dependent check of send-boxes/binding/condition-false is ESS-CF-INVOCATION, which Spindle's target reports unsupported and line 211 accepts, so the conform test stays green if the condition is ignored
- file: docs/design/spindle.md
  line: 406
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the unmet-binding-input decision row lists a missing required field and a failed conversion but not the Unknown where-condition now reported through the same Unmet path
```

---
format: aep.planning-md/3
id: story:kernel-command-responses
kind: story
status: draft
title: Command responses, executed and served
relations:
- serves: vision:spindle
- decomposes: epic:downstream-kernel-constructs
- depends_on: story:serve-what-runs
scope:
- confidence: cited
  path: crates/spindle-ingress/src/answer.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
- confidence: inferred
  path: crates/spindle-kernel/src/execute.rs
- confidence: inferred
  path: crates/spindle-kernel/src/mint.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/responses.rs
revision: 7
---
# Command responses, executed and served

## Outcome

The kernel runs an outcome that declares `returns: true`: it answers the command's declared
`response:` fields with values of their declared types, chosen by the runtime as ESS leaves them
to the implementation, and a field listed in `one_time_response:` is disclosed only in that
invocation's response: never stored, published, put in an error or a view, and never originated
twice. The HTTP ingress answers such an outcome with `response` in ESS's envelope, as
`ess generate --kind openapi` publishes it. Today `crates/spindle-kernel/src/supported.rs`
(`shape`) refuses it as "a command response" and `crates/spindle-ingress/src/surface.rs` (`route`)
refuses the route as one "which answers with the command's response"; `story:serve-what-runs`
answers it `501`.

Requested by a downstream specification probe (2026-11-05, 44 commands): 3 commands are refused
first on this construct, one of them with a one-time field. Reproduced with a neutral fixture.

ESS semantics: ESS 0.86.0's interpreter, `crates/verify/ess-conformance/src/interpret/response.rs`
(the implementation-owned choice and its checks); `docs/design/one-time-response-values.md` at ess
0.86.0.

## Acceptance

Fixture: a notebook whose `ShareNotebook` command returns `{link: String, share_code: String}`
with `one_time_response: [share_code]`.

- `a_returning_outcome_answers_its_typed_response` — the result carries both fields, each a value
  of its declared type.
- `a_one_time_field_appears_nowhere_else` — the share code is in no stored row, event, view row or
  later response.
- `a_second_invocation_originates_a_fresh_one_time_value`.
- `the_ingress_answers_the_response_in_the_envelope` — `POST` answers the outcome's status with
  `response` beside `outcome` and `published`, matching the published OpenAPI schema.
- `responses_agree_with_the_interpreter` — for the same steps, the interpreter's response checks
  accept every response the kernel answers.

## Scope

- `crates/spindle-kernel/src/supported.rs` — cited: the `returns`/`one_time_response` refusal in
  `shape`.
- `crates/spindle-kernel/src/execute.rs` — inferred: the response in `Executed`.
- `crates/spindle-kernel/src/mint.rs` — inferred: minting the response values.
- `crates/spindle-ingress/src/surface.rs` — cited: the refusal in `route`.
- `crates/spindle-ingress/src/answer.rs` — cited: `envelope`.
- `crates/spindle-kernel/tests/responses.rs` — inferred, new.

## Depends on

- `story:serve-what-runs` — touches `surface.rs` and turns this refusal into a `501`; this story
  then serves the route.

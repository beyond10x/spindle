---
format: aep.planning-md/3
id: story:refusal-payload-complete
kind: story
status: implemented
title: A refusal answers every error field its schema requires
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-ingress/src/answer.rs
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T09:08:51Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-06T09:08:51Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-06T16:03:00Z", actor: "human:timo", revision: 5, decided_on: {"recorded":{"test_result":1}}}
---
# A refusal answers every error field its schema requires

## Outcome

A refused outcome whose error declares fields answers over HTTP with a `payload` holding every
field the published OpenAPI schema requires, as ESS's served application answers it. Today
`crates/spindle-ingress/src/answer.rs` writes `payload` only when the kernel found a value, and the
kernel fills only the fields `error_payload:` sets, so a refusal can answer with no payload or a
partial one.

## Acceptance

- `a_refusal_answers_the_payload_its_schema_requires` — a neutral fixture with an error declaring
  two fields, one set by `error_payload:`: the answer validates against the published schema, or
  the specification is refused at start naming the unfilled field, whichever ESS's interpreter
  does.
- `refusal_payloads_agree_with_the_interpreter`.

## Origin

Pre-existing; inferred from code by the kernel-command-responses unit (wave 8), not yet run. A
consumer's UI suite fails on refused updates whose refusal carries an error body. Related:
`story:ingress-error-object-bodies`.

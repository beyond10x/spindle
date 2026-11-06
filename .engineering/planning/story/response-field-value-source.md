---
format: aep.planning-md/3
id: story:response-field-value-source
kind: story
status: active
title: 'A payload or sets: value read from the returned response'
relations:
- depends_on: story:structured-response-values
- serves: vision:spindle
- depends_on: story:one-test-binary-per-crate
scope:
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/response-source
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/response-source-baseline
- confidence: inferred
  path: crates/spindle-kernel/src/execute.rs
- confidence: inferred
  path: crates/spindle-kernel/src/supported.rs
- confidence: inferred
  path: crates/spindle-kernel/src/values.rs
revision: 8
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T15:02:53Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"review_outcome":4}}}
- {from: "proposed", to: "active", at: "2026-11-06T15:02:53Z", actor: "human:timo", revision: 8, decided_on: {"recorded":{"review_outcome":4}}}
---
# A payload or `sets:` value read from the returned response

## Outcome

An outcome whose event payload or `sets:` reads `response.<field>` (ESS `ResolvedPayloadValue::ResponseField`)
runs with the value the same invocation answers in that response field, as ESS's interpreter
reads it (ESS 0.88.0 `crates/verify/ess-conformance/src/interpret/execute/determined.rs:419`,
`interpret/execute.rs:2610`). Today `crates/spindle-kernel/src/supported.rs:282` (`value`, the
`other =>` arm) refuses it as "the value source response field `<field>`", so the command answers
501 over HTTP.

## Acceptance

All three run on one neutral fixture, `crates/spindle-cli/tests/fixtures/response-source/`: a
creating command whose outcome answers a generated `Uuid` `id` in its response, publishes it in its
event payload and sets it on the created row.

- `a_payload_reads_the_returned_response_field`: one request; the published event's `id` equals the
  response's `id`, and the answer passes ESS's `direct_response::Observation::compare`. Before: the
  route answers 501 naming "the value source response field `id`".
- `sets_reads_the_returned_response_field`: on an empty store, one request, then a read of the
  created row through the fixture's view; the row's field equals the `id` that request answered.
  A second request answers a different `id`, and its row holds that one.
- `capabilities_derive_one_mint_for_the_response_id`: `b10x-spindle capabilities` on the fixture
  lists `identity.mint` once, its reason naming the response field `id`, and otherwise the same
  requirements it lists for the sibling fixture `crates/spindle-cli/tests/fixtures/response-source-baseline/`,
  which answers the same response and reads it in neither the payload nor `sets:`.

## Order

- `depends_on` `story:structured-response-values`: the response it reads.
- `depends_on` `story:one-test-binary-per-crate`: the scenarios are one module,
  `response_source`, in the spindle-cli test binary that story builds.
- Shares `supported.rs` with active `story:outcome-scoped-refusal`; scope keeps them in separate
  waves.
- Shares `crates/spindle-kernel/src/supported.rs` (the `value` match) with active
  `story:kernel-related-row-values` and `story:kernel-set-outcomes`, and `supported.rs` alone with
  `kernel-row-set-guards`, `kernel-secondary-effects` and `state-change-conditions`; `values.rs`
  with `kernel-related-row-values` and `kernel-set-outcomes`; `execute.rs` with
  `structured-response-values` and `kernel-set-outcomes`. These are recorded in the scope, and
  `aep plan artifact waves` puts none of them in its wave.

## Origin

A consumer serving a specification as a UI test backend (2026-11-06, on main 62b97a9): one create
route answered 501 with this refusal, 8 calls in one UI run. Follows
`story:structured-response-values`.

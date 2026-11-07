---
format: aep.planning-md/3
id: story:http-outcome-bodies
kind: story
status: implemented
title: Success outcome bodies an HTTP binding declares
relations:
- depends_on: story:outcome-scoped-refusal
- serves: vision:spindle
- depends_on: story:one-test-binary-per-crate
scope:
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/outcome-bodies
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/outcome_bodies.rs
- confidence: cited
  path: crates/spindle-docs/src/status.rs
- confidence: inferred
  path: crates/spindle-ingress/src/answer.rs
- confidence: cited
  path: crates/spindle-ingress/src/console.rs
- confidence: cited
  path: crates/spindle-ingress/src/listener.rs
- confidence: cited
  path: crates/spindle-ingress/src/router.rs
- confidence: inferred
  path: crates/spindle-ingress/src/surface.rs
- confidence: cited
  path: crates/spindle-ingress/src/wire.rs
revision: 16
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T15:02:53Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"review_outcome":5}}}
- {from: "proposed", to: "active", at: "2026-11-06T15:02:53Z", actor: "human:timo", revision: 8, decided_on: {"recorded":{"review_outcome":5}}}
- {from: "active", to: "implemented", at: "2026-11-07T01:57:24Z", actor: "human:timo", revision: 16, decided_on: {"recorded":{"test_result":1,"review_outcome":6,"verification":1}}}
---
# Success outcome bodies an HTTP binding declares

## Outcome

A command outcome that is not a refusal, whose `ess-http/1` binding lists the body `response`,
`response.<field>` or `none` (ESS 0.88.0 `crates/specify/ess-http/src/lib.rs:490`, `Body`;
`docs/design/http-presentation-binding.md`, "Response body per outcome"), is answered with that
body and the bound status. Today `crates/spindle-ingress/src/surface.rs:851-858` refuses the route
as "`<outcome>` answered other than with its status and the envelope", so it answers 501.

Decision on scope: the request names `created` on two create routes and does not say which body
kind they bind, so this story serves the three success body kinds, which share one refusal site.
Out of it, still refused by name: the error-object body (`story:ingress-error-object-bodies`),
a non-default response encoding, bound headers and a redirect.

## Acceptance

One neutral fixture, `crates/spindle-cli/tests/fixtures/outcome-bodies/`, binds one creating
command per body kind. For each, before: the route answers 501 with the refusal above.

- `a_response_body_is_served_bare`: `body: response` answers the bound status,
  `content-type: application/json`, and a body equal to the `response` member the envelope would
  carry, which validates against the schema `ess generate --kind openapi` publishes for that
  status.
- `a_response_field_body_is_served_bare`: `body: response.id` answers the bound status and the
  value of `id` alone, valid against the published schema.
- `a_none_body_answers_no_content`: `body: none` answers the bound status with an empty body and
  no `content-type`.
- `an_unserved_outcome_presentation_is_refused_by_name`: a route of the fixture binding a header on
  its outcome answers 501 naming the outcome and "headers"; one binding a redirect names
  "redirect"; one binding a non-default encoding names "encoding". After the three requests, the
  fixture's view lists no rows.

## Order

- `depends_on` `story:outcome-scoped-refusal`: both rewrite the refusal at `surface.rs:851-858`.
  When that story has landed, the refusal in the last acceptance fires when the outcome is chosen,
  as it rules; the acceptance reads the answer to such a request and holds under either timing.
- `depends_on` `story:one-test-binary-per-crate`: the scenarios are one module, `outcome_bodies`,
  in the spindle-cli test binary that story builds.
- Shares `crates/spindle-ingress/src/surface.rs` and `answer.rs` with active
  `story:fixed-caller-per-grant` and `story:refusal-payload-complete` (both merged in wave 10),
  `story:ingress-request-log`, and `surface.rs` with `story:live-topology`. These are recorded in
  the scope, and `aep plan artifact waves` puts none of them in its wave.

## Origin

A consumer serving a specification as a UI test backend (2026-11-06, on main 62b97a9): two create
routes answered 501 with this refusal, 6 calls in one UI run.

## Scope

Delivered in wave 12 (implementor's confirmation table, commits 5315f8f, 1404dd0):

- `crates/spindle-ingress/src/surface.rs` — inferred, confirmed: `Route::bodies`, `Route::body`, `unwritten()` (the refusal was at about line 976 at 920e475, not 851-858).
- `crates/spindle-ingress/src/answer.rs` — inferred, confirmed: `Answer.body: Option<Value>`, `bare()`, one wire-name mapping for the envelope and the bare bodies.
- `crates/spindle-ingress/src/wire.rs` — new: a declared value rewritten to ESS's wire names and spellings.
- `crates/spindle-ingress/src/router.rs`, `listener.rs`, `console.rs`, `lib.rs` — not in the scope, needed: the empty body and its headers.
- `crates/spindle-cli/tests/fixtures/outcome-bodies` — inferred, confirmed; plus `adversary-outcome-bodies`.
- `crates/spindle-cli/src/cli.rs`, `crates/spindle-docs/src/status.rs`, `website/` — help text, status item, generated pages.

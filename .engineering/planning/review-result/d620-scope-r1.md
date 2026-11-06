---
format: aep.planning-md/3
id: review-result:d620-scope-r1
kind: review-result
status: active
title: D620 stories, scope critic, round 1
relations:
- reviews: story:response-field-value-source
- reviews: story:http-outcome-bodies
revision: 1
---
needs-revision

**Findings**

1. `story:http-outcome-bodies` overlaps `story:ingress-error-object-bodies`. Both are drafts and both claim the error-object body.
   - The first lists "an error object with its member" in its Outcome and `none`/error object in `a_bare_response_body_is_served_as_bound`.
   - The second says "A refused outcome whose `ess-http` binding maps it to a status and an error object ... is answered with that status and a body built from the outcome's error fields", with acceptance `a_bound_error_outcome_answers_its_status_and_error_object`.
   - Two items would both be marked done for one outcome. Fix: either drop the error-object body from `story:http-outcome-bodies` and add `depends_on`/`relates` to the existing story, or fold the existing story into it and supersede that story.

2. `story:http-outcome-bodies` reaches past the request. The request is for `created`, a success outcome on two create routes, and the story has no cited body kind for it.
   - The story claims every `Body` kind (`Response`, `ResponseField`, `None`, `Error`), response encodings, headers and redirects. Apart from the error object (finding 1), nothing in the request names any of these.
   - The refusal at `crates/spindle-ingress/src/surface.rs:851-858` is one shared message for all of them, so serving all of them is defensible. Narrowing is the alternative: claim only the body and header forms the two routes bind, and state in the body that the rest stays refused by name.
   - Either way the story should say which it is. Right now it silently widens the request. The third acceptance bullet ("still not served is refused by name") is the honest part and should stay.
   - Citation: `.engineering` store, story body "Outcome" and "Acceptance"; code `crates/spindle-ingress/src/surface.rs:851-858`.

3. `story:response-field-value-source` is traceable to request (2). The refusal text comes from `crates/spindle-kernel/src/supported.rs:282` (`other => Some(format!("the value source {}", other.describe()))`), and `ResponseField` is not handled in `value` at lines 252-283. No overlap found.
   - `story:structured-response-values` covers response field types, not payload or `sets:` reading `response.<field>`. `story:response-witness-types` is also about types.
   - The `depends_on structured-response-values` edge is right.
   - Its acceptance has one bullet the request does not ask for: "Capability derivation names the mint the response field needs and no other". It follows from the AGENTS.md derivation rule, so I do not count it as reach.

Out of my lane, which I did not let set the verdict:
- Both stories lack a `serves vision:spindle` relation, which sibling stories carry.
- Story 1's scope omits `crates/spindle-kernel/src/execute.rs`, which it likely touches to order the response before the event payload.
- Both stories list scope as `inferred`.

**What you read / could not establish**
- Both stories in full, and the full `aep plan artifact list` output.
- The overlap candidates in full: `ingress-error-object-bodies`, `response-witness-types`, `structured-response-values`, `kernel-command-responses`, `refusal-payload-complete`, `outcome-scoped-refusal`, `serve-what-runs`.
- Code in `spindle-wave10`: `surface.rs` 800-900, `supported.rs` `value` and the refusal sites.
- The ESS `Body` enum and `docs/design/http-presentation-binding.md` in the cargo checkouts, to confirm `response.<field>` exists as a body form.
- Promises extracted from the request: 2 (the response-field value source; non-envelope outcome bodies). Both trace to a story, so there are no gaps.
- Could not establish:
  - which cargo checkout is ESS 0.88.0. `ess-http/src/lib.rs:490` matches `Body` in several checkouts, and `determined.rs:419` and `execute.rs:2610` exist only in older ones I opened, so I could not verify those line citations.
  - which `Body` kind the consumer's `created` outcome binds. The request quote does not say.
  - the source request text beyond the two quoted refusals.

```findings
[{"file": ".engineering/planning/story/http-outcome-bodies.md", "category": "scope", "severity": "blocker", "message": "claims the error-object body, which story:ingress-error-object-bodies (draft) already claims; two items for one outcome"},
 {"file": ".engineering/planning/story/http-outcome-bodies.md", "category": "scope", "severity": "warning", "message": "claims every body kind, response encoding, headers and redirect where the request names only the created outcome of two create routes; widening is not recorded as a decision (crates/spindle-ingress/src/surface.rs:851-858)"}]
```

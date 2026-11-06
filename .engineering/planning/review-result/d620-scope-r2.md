---
format: aep.planning-md/3
id: review-result:d620-scope-r2
kind: review-result
status: active
title: D620 stories, scope critic, round 2
relations:
- reviews: story:response-field-value-source
- reviews: story:http-outcome-bodies
revision: 1
---
approve

Both round-1 findings are fixed. Nothing I found blocks either story.

**Round 1 findings**
1. Fixed. `story:http-outcome-bodies` (revision 5) no longer claims the error-object body. It names `story:ingress-error-object-bodies` as out of scope and "still refused by name". The remaining acceptance lines cover only `response`, `response.<field>` and `none`.
2. Fixed. The story now has a "Decision on scope" paragraph. It serves the three success body kinds because they share the one refusal site at `crates/spindle-ingress/src/surface.rs:851-858`. It lists what stays refused: error object, non-default encoding, headers, redirect. The last acceptance line (`an_unserved_outcome_presentation_is_refused_by_name`) checks the refusals.
3. Round 1 had no finding for `story:response-field-value-source`, and its body is unchanged in substance. Scope now includes `execute.rs`, and a `serves vision:spindle` relation is present on both stories. These were the round-1 out-of-lane notes.

**Fresh pass**
- **No overlap.** `story:response-field-value-source` addresses request (2). The refusal is `supported.rs:282`, where the `other =>` arm of `value` produces "the value source …". No other story claims it:
  - `story:structured-response-values` and `story:response-witness-types` cover response field types.
  - `story:identity-mint-types-and-fallback` covers identity types and fallbacks.
  - `story:kernel-related-row-values` is a different value source.
- **Request (3) traces to `story:http-outcome-bodies`.** The refusal text matches the code at `surface.rs:852-858`: `Body != Envelope`, encoding, headers and redirect all return that one message.
- **Different refusal site, not an overlap.** The `retains_result` refusal at `surface.rs:840-845` ("which answers with the command's response") is separate from request (3)'s message, and neither story claims it.
- **Note on `ingress-error-object-bodies`.** The exclusion is only in prose. `story:http-outcome-bodies` has no `relates` edge to it, so the graph doesn't show the boundary. This is a note and doesn't set the verdict.
- **Note on refusal wording.** The last acceptance line expects the refusal to name "headers", "redirect" and "encoding". The current code gives one undifferentiated message, so this story would introduce the distinct wording. It is in the story's own acceptance, so I count it as claimed rather than reach.

**Promises traced:** the request has 2 promises. Both trace to a story, so there are no gaps.

**What I read**
- Both stories in full.
- `review-result:d620-scope-r1`.
- The full artifact list.
- `story:ingress-error-object-bodies` and `story:identity-mint-types-and-fallback`.
- `surface.rs` lines 835-870.
- `supported.rs` lines 275-285.

**What I could not establish**
- The ESS 0.88.0 line citations (`lib.rs:490`, `determined.rs:419`, `execute.rs:2610`), as in round 1.
- Which body kind the consumer's `created` outcome binds. The story's decision on scope covers that.
- The consumer's original request text beyond the two quoted refusals.

```findings
[{"file":".engineering/planning/story/http-outcome-bodies.md","line":0,"category":"scope","severity":"note","message":"The error-object exclusion is recorded only in prose; no relates edge to story:ingress-error-object-bodies makes the boundary visible in the graph"}]
```

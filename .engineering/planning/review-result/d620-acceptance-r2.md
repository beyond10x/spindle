---
format: aep.planning-md/3
id: review-result:d620-acceptance-r2
kind: review-result
status: active
title: D620 stories, acceptance critic, round 2
relations:
- reviews: story:response-field-value-source
- reviews: story:http-outcome-bodies
revision: 1
---
approve

All five round-1 findings are fixed. Both stories are now at revision 4 and 5, still draft, and I found no blockers. I read the stories and both code sites, but ran none of the named scenarios.

**Round 1 findings**
1. Capability derivation, third bullet of `response-field-value-source`: fixed. It is now `capabilities_derive_one_mint_for_the_response_id`. It names the command (`b10x-spindle capabilities` on the fixture), the output (`identity.mint` listed once), the reason (names the response field `id`) and a comparison. Warning 1 below covers one gap.
2. `sets_reads_the_returned_response_field`: fixed. It names the fixture, starts on an empty store and reads the row through the fixture's view. It compares the row with the `id` that request answered, and a second request must give a different `id` in its own row.
3. Unbounded refusal in `http-outcome-bodies`: fixed. It is now `an_unserved_outcome_presentation_is_refused_by_name`. It names three constructs (headers, redirect, encoding) and the answer (501 naming the outcome and the construct).
4. Joined headers and redirect, and the uncovered encoding: fixed. Headers, redirect and encoding are separate routes in that one scenario, each named. All three are explicitly out of scope, so none is left unaccepted.
5. Unstated comparison against the OpenAPI schema: fixed. The status, the `content-type`, the body equal to the envelope's `response` member and the schema validity are each stated.

**New findings**
- The 501 premise holds. `surface.rs:851-858` returns an `Err`. The route is then kept and marked `Unserved::Ingress`, which `surface.rs:88-92` documents as answering 501 before anything is read or written. The refusal text and `supported.rs:282` match what the stories quote. ESS `ir.rs:1881` describes the value as "response field `<field>`", so the quoted text "the value source response field `id`" is right.
- Warning 1: the `capabilities` scenario's last clause compares with "the same fixture without the payload and `sets:` reads". No such second fixture is named or scoped. An implementer has to invent it, and the scope lists only `fixtures/response-source`. Name it, for example as a sibling fixture directory, or state the expected full requirement list.
- Warning 2: "Nothing is written" in the refusal scenario names no way to observe it. Say which store read shows it.
- Note: both stories still list their scopes as "inferred". That belongs to the scope critic, not this role.

**What I read:** both stories in full, `review-result:d620-acceptance-r1`, `surface.rs` lines 86-118 and 835-865, `supported.rs` lines 270-290, and the ESS `ir.rs` line for the description text.

**What I could not establish:**
- Whether `story:outcome-scoped-refusal` timing really leaves the last scenario true either way. That is a design question.
- The 8-call and 6-call origin counts, and "main 62b97a9".
- Whether `identity.mint` is derived once, and with that reason text, when both the payload and `sets:` read the response field. I did not trace `derive.rs`.
- Whether `Observation::compare` accepts the fixture's response.

```findings
[{"file":".engineering/planning/story/response-field-value-source.md","line":0,"category":"acceptance","severity":"warning","message":"capabilities_derive_one_mint_for_the_response_id compares against 'the same fixture without the payload and sets: reads', a second fixture that is neither named nor in scope; name it or state the expected full requirement list"},
{"file":".engineering/planning/story/http-outcome-bodies.md","line":0,"category":"acceptance","severity":"warning","message":"an_unserved_outcome_presentation_is_refused_by_name says 'Nothing is written' without naming how that is observed (which store read)"}]
```

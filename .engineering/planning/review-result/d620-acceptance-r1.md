---
format: aep.planning-md/3
id: review-result:d620-acceptance-r1
kind: review-result
status: active
title: D620 stories, acceptance critic, round 1
relations:
- reviews: story:response-field-value-source
- reviews: story:http-outcome-bodies
revision: 1
---
needs-revision

Findings:
- story:response-field-value-source — the third bullet, "Capability derivation names the mint the response field needs and no other", has no scenario name. It names no fixture, no command and no output to read the derived requirement from. "The mint the response field needs" is defined only by the author's intent, so one reviewer's "no other" can differ from another's. Name a scenario (for example `capabilities` output on the fixture lists exactly the id mint, with its reason), as the first two bullets do. — `aep plan artifact show story:response-field-value-source`, Acceptance, bullet 3.
- story:response-field-value-source — `sets_reads_the_returned_response_field` says only "the stored row holds the answered value". It names no fixture and no way to read the row. Unlike bullet 1, it does not say that the response and the stored value are the same one. A stored value that differs from the answered one, or a pre-existing row, would also match. It needs the neutral fixture and the comparison with the answered `id`. — same artifact, Acceptance, bullet 2.
- story:http-outcome-bodies — the third bullet, "A binding construct still not served is refused by name when the surface is built", is unbounded and has no scenario name. "Still not served" depends on what the implementation covers when it finishes, so it can never be shown false. It needs a named construct and a named refusal text or exit status. The current refusal is the `Err` at `crates/spindle-ingress/src/surface.rs:851-858`, which already fires at build time. — same artifact, Acceptance, bullet 3.
- story:http-outcome-bodies — `bound_headers_and_redirect_are_served` joins two independent outcomes (headers and redirect) in one scenario, so one can pass while the other fails. The Outcome also lists "a response encoding", and no scenario covers it. That construct is neither accepted nor named as refused. — same artifact, Outcome and Acceptance bullet 2.
- story:http-outcome-bodies — bullet 1 says "each answer matches what `ess generate --kind openapi` publishes". It does not say which property is compared (status, content type, body shape or values). The one-fixture-per-kind list is checkable once that is stated. — same artifact, Acceptance, bullet 1.

Out of my lane: nothing found. Both stories' scopes are marked "inferred", which is for the design and scope critics.

What I read
- I read the full bodies of both ids given, 2 of 2.
- I checked the cited code in the wave10 tree and in ESS bd6ecb3.
  - The refusal text at `surface.rs:851-858` exists, and so does `Body` at `ess-http/src/lib.rs:490`.
  - `determined.rs:419` and `execute.rs:2610` handle `ResponseField`.
  - `direct_response::Observation::compare` exists at `direct_response.rs:210`.
  - `http-presentation-binding.md` exists.
  - `supported.rs` has the "the value source …" refusal at line 282. Lines 221-240 already contain response-field-specific text that I did not trace.

What I could not establish
- I did not run `aep plan artifact kinds` or `aep plan artifact lifecycle story`.
- I did not confirm that the quoted refusal "the value source response field `<field>`" is the exact string emitted today.
- I did not confirm that the stated 501 behaviour, or the "8 calls" and "6 calls" counts from the origin report, are real. Those are origin claims.
- I did not confirm which `main` the claim "on main 62b97a9" refers to.

```findings
[{"file": ".engineering/planning/story/response-field-value-source.md", "category": "acceptance", "severity": "blocker", "message": "Third bullet 'Capability derivation names the mint the response field needs and no other' has no scenario name, fixture or observable output; 'the mint it needs' is undefined"},
 {"file": ".engineering/planning/story/response-field-value-source.md", "category": "acceptance", "severity": "blocker", "message": "sets_reads_the_returned_response_field names no fixture and does not tie the stored row to the answered value or to a before state"},
 {"file": ".engineering/planning/story/http-outcome-bodies.md", "category": "acceptance", "severity": "blocker", "message": "'A binding construct still not served is refused by name' is unbounded, unnamed and unfalsifiable; no construct or refusal text is given (crates/spindle-ingress/src/surface.rs:851-858)"},
 {"file": ".engineering/planning/story/http-outcome-bodies.md", "category": "acceptance", "severity": "blocker", "message": "bound_headers_and_redirect_are_served joins two independent outcomes, and the response encoding named in the Outcome has no scenario"},
 {"file": ".engineering/planning/story/http-outcome-bodies.md", "category": "acceptance", "severity": "blocker", "message": "'matches what ess generate --kind openapi publishes' does not say which properties are compared"}]
```

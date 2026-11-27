---
format: aep.planning-md/3
id: review-result:wave30-acc-r2
kind: review-result
status: active
title: Plan critic (acceptance), round 2, on wave 30
relations:
- reviews: story:view-rows-absent-optional-fields
- reviews: story:view-filter-nested-element-quantifier
- reviews: story:view-filter-map-value-order
- reviews: story:error-payload-generated-fields
- reviews: story:unknown-instance-unsourced-unserved
- reviews: story:create-on-held-identity-unavailable
- reviews: story:decimal-identity-any-spelling
- reviews: story:look-ahead-asks-only-where-it-matters
- reviews: story:related-via-nested-input
- reviews: story:events-cursor-across-restart
- reviews: story:peer-connect-test-steady
- reviews: story:rally-example-reads-stdin
revision: 1
---
approve

All six findings from round 1 are fixed in 4547ef74:
- **view-rows-absent-optional-fields**: the read path is now named (`HostTarget::query_view`), and a new case checks that a row read through a served view (`Host::listed_with`) still leaves the field out.
- **unknown-instance-unsourced-unserved**: the 501 now names the outcome and the field. A new case checks that the route's other outcomes are still served.
- **error-payload-generated-fields**: the seeded case keeps only the statement that shows the change.
- **decimal-identity-any-spelling**: `an_established_decimal_identity_is_found_whatever_its_spelling` covers `Host::establish`. It fails before the work, because a row established as `1.0` is not found by a command naming `1`.
- **peer-connect-test-steady**, item 4: it now names the load (an average of at least 40) and where the runs are recorded (`test_result` evidence on the story). That evidence kind exists in the store.
- **peer-connect-test-steady**, item 1: it now says each case of the two tests listens on a loopback host of its own.

Reading all 12 stories again found nothing new.

**What I read:** all 12 of 12 stories, whole bodies (`aep plan artifact show story:<id>`). I also read `review-result:wave30-acc-r1`, the diff of 4547ef74 for the stories, `aep plan artifact kinds`, the help of `aep plan artifact evidence`, the wave page, `crates/spindle-spec/src/lib.rs` (`LoadError` and `compile`), and in ESS d7833ee: `related_guard.rs:1665-1690`, `Command::input_field` (`command.rs:2891`), and the `Display` code for `ValidationError` and `ValidationCode` (`ess-primitives/src/error.rs:387,472,862`).

**What I could not establish:**
- **story:related-via-nested-input**, settled since round 1: ESS writes the code in the error text as `[undeclared_reference] <location>: ...`, not as `UndeclaredReference`. The test has to look for the snake_case spelling. The acceptance can still be checked, so this is not a finding. ESS's lookup is flat (`input.iter().find(|field| field.name == name)`), so it does reject a nested `via:`, as the story says.
- **story:error-payload-generated-fields**, settled since round 1: `an_unsourced_error_field_draws_as_ess_draws` can be built on a minted unsourced field (`Unsourced::Minted`). It does not need an `unknown_instance:` refusal, so U3's first story does not block it.
- **story:peer-connect-test-steady**, item 1 still ends with the clause about a peer at `127.a.b.c:P` printing its listening line. That clause was already true before the work. It sits beside the new statement that shows the change and does not weaken it, so it is not a finding.
- **Out of my lane (scope):** in **story:unknown-instance-unsourced-unserved**, the Scope "Also likely" line still says "the two acceptance tests" while the Acceptance now has four, and the Outcome still says it was read at ESS 0.87.0.
- No person, organisation, host or home path appears in any of the 12 bodies.

```findings
[]
```

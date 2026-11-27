---
format: aep.planning-md/3
id: review-result:wave30-sco-r2
kind: review-result
status: active
title: Plan critic (scope), round 2, on wave 30
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

**What I read:** the 12 stories through `aep plan artifact show`, the round-1 review `review-result:wave30-sco-r1` and the revision commit 4547ef74. For overlaps I also read the wave-29 page, `aep plan artifact list --kind story` and `story:command-input-decimal-text`. The wave page's Selection list and each story's Outcome count as the parent.

**Count:** 26 promises, all 26 traced to an item.
- The 12 wave-page selections are each claimed by exactly one story.
- The 12 Outcomes are each claimed by Acceptance, Scope or a recorded decision.
- The 2 extra promises in the decimal-identity Outcome (`Binary64`, and a `Decimal` field keeping its spelling) are claimed too.

**The three round-1 findings are fixed:**
- **`story:view-rows-absent-optional-fields`:** D1 A no longer promises a report to ESS.
- **`story:decimal-identity-any-spelling`:** the Outcome now names `Binary64`. Acceptance claims `a_binary64_identity_names_one_row_whatever_its_spelling`, and adds a test for `Host::establish` to match D2 A.
- **`story:events-cursor-across-restart`:** Scope now lists `a_head_cursor_from_before_a_restart_reads_the_events_after_it` among the tests to add, without "optional".

**Checked and not findings:**
- No two stories claim the same outcome. `story:command-input-decimal-text` covers decimal command input, not identity keying.
- The narrowings are all written in Decided sections, so none is silent:
  - view-rows D1 A
  - view-filter-map D2 A (only a `Map` field of the row is admitted, not a `Map` view parameter)
  - unknown-instance D1 A (an `Optional` field stays absent)
  - create-on-held D1 A (`kind()` and the message stay)
  - related-via D1 B
- Each piece of work beyond an Outcome comes from a recorded decision:
  - the `, unavailable` replay line (create-on-held D3 B)
  - the self-connect refusal in `Host::connect` (peer-connect D1 A)
  - the `--stdin` flag and its refusal of served-only flags (rally D1 A)
  - the seeded-mint test (error-payload D1 A)
- No story body or the wave page names a person, organisation, host or home path.

**What I could not establish:**
- **Out of my lane, for the acceptance critic:**
  - `story:unknown-instance-unsourced-unserved` still cites "read at ESS 0.87.0" in its Outcome, though the pinned release is 0.99.1.
  - The Scope of that story does not place `an_optional_unsourced_field_of_an_unknown_instance_refusal_is_left_out` in any test file. The Scope of `story:decimal-identity-any-spelling` does not place `an_established_decimal_identity_is_found_whatever_its_spelling` either.
- **Out of my lane, a note for the design critic:** in `story:create-on-held-identity-unavailable`, the Outcome reads as if every answer becomes "unavailable". Only D1 A says that the CLI and HTTP `kind` stay `undeclared`.

```findings
[]
```

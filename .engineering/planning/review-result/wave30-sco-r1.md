---
format: aep.planning-md/3
id: review-result:wave30-sco-r1
kind: review-result
status: active
title: Plan critic (scope), round 1, on wave 30
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
needs-revision

story:view-rows-absent-optional-fields — Decision D1 A promises "The gap between ESS's interpreter and its published contract is reported to ESS", but no Acceptance line, Scope line or outbound-claim artifact covers that report, so the story can be marked implemented without it being sent; either add an acceptance line for it or remove the promise — .engineering/planning/story/view-rows-absent-optional-fields.md:97-98
story:decimal-identity-any-spelling — the Outcome and Acceptance promise only `Decimal` identities, but Scope adds `a_binary64_identity_names_one_row_whatever_its_spelling` and changes how existing `Binary64` rows are keyed (and D1 A writes every number in one spelling), so either the Outcome names `Binary64` and Acceptance claims it, or that test is dropped — .engineering/planning/story/decimal-identity-any-spelling.md:78
story:events-cursor-across-restart — Acceptance requires `a_head_cursor_from_before_a_restart_reads_the_events_after_it` (line 42), but Scope lists it as "optional", so the Scope does not fully cover the Acceptance and should list it as required — .engineering/planning/story/events-cursor-across-restart.md:59

**What I read:**
- The wave page, read first as the parent: 12 promises from its Selection list.
- All 12 stories in full with `aep plan artifact show`.
- `aep plan artifact kinds` and `aep plan artifact relations`, `aep plan artifact graph`, `aep plan artifact list --kind story` and `aep plan artifact list --kind outbound-claim`.
- `story:integer-mint-on-ess-ticks` and the wave-29 page, to look for overlapping claims.

**Count:** 25 promises in total: the 12 on the wave page plus 13 in the stories (12 Outcomes and the "reported to ESS" promise in view-rows' D1 A).
- All 12 wave-page promises are claimed exactly once.
- 23 of the 25 are fully covered by Acceptance and Scope. The other two are view-rows' "reported to ESS" (nothing claims it) and the events cursor's head-cursor test (Scope marks it optional).

**Checked and not findings:**
- No two stories claim the same outcome. `story:integer-mint-on-ess-ticks` covers Integer tick values, not draws from an error payload. The two U3 stories split unknown_instance fields that have no source from minted declared-error fields.
- Two narrowings are written down in the stories' Decided sections, so they are not silent: view-rows D1 A (the conformance target changes, served views keep the published contract) and related-via D1 B (a test that pins ESS's rejection, no kernel change).

**What I could not establish:**
- These belong to the acceptance lane:
  - Several Outcomes name old ESS versions: view-rows says 0.86.0, unknown-instance says 0.87.0, error-payload says 0.86.0. The pin is 0.99.1.
  - unknown-instance's Acceptance still says "or, with `story:outcome-scoped-refusal`", but that story is already implemented.
  - create-on-held's D3 B adds a `, unavailable` replay output, and no Acceptance line checks it.
  - The look-ahead story has no "Decided when scoped" section, although the brief says every story has one.
- This belongs to the parallel-safety lane: view-filter-nested's Scope infers a doc-comment edit in `crates/spindle-host/src/host.rs:1974-1977`. The wave page's Surfaces section gives `host.rs` only to U6 and U7, and wave 29 also changes `host.rs`.

```findings
- file: .engineering/planning/story/view-rows-absent-optional-fields.md
  line: 97
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "Decision D1 A promises \"The gap between ESS's interpreter and its published contract is reported to ESS\", but no Acceptance line, Scope line or outbound-claim artifact covers that report, so the story can be marked implemented without it being sent; either add an acceptance line for it or remove the promise"
- file: .engineering/planning/story/decimal-identity-any-spelling.md
  line: 78
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome and Acceptance promise only `Decimal` identities, but Scope adds `a_binary64_identity_names_one_row_whatever_its_spelling` and changes how existing `Binary64` rows are keyed (and D1 A writes every number in one spelling), so either the Outcome names `Binary64` and Acceptance claims it, or that test is dropped"
- file: .engineering/planning/story/events-cursor-across-restart.md
  line: 59
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "Acceptance requires `a_head_cursor_from_before_a_restart_reads_the_events_after_it` (line 42), but Scope lists it as \"optional\", so the Scope does not fully cover the Acceptance and should list it as required"
```

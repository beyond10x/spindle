---
format: aep.planning-md/3
id: review-result:wave25-scope-r1
kind: review-result
status: active
title: Plan critic (scope), round 1, on wave 25
relations:
- reviews: story:planning-only-prs-skip-tests
- reviews: story:test-step-within-budget
- reviews: story:wasm-lanes-within-budget
- reviews: story:decisions-rules-on-input-predicates
- reviews: story:guard-over-absent-optional-input
- reviews: story:identity-mint-types-and-fallback
- reviews: story:reset-reapplies-seed
- reviews: story:world-row-integer-fields
revision: 1
---
needs-revision

Per story: planning-only-prs-skip-tests needs-revision; test-step-within-budget needs-revision;
wasm-lanes-within-budget approve; decisions-rules-on-input-predicates needs-revision;
guard-over-absent-optional-input approve; identity-mint-types-and-fallback approve;
reset-reapplies-seed approve (the readiness test moved to Not covered with `story:seed-readiness`
named; the Outcome no longer promises it); world-row-integer-fields approve.

```findings
- file: .engineering/planning/story/planning-only-prs-skip-tests.md
  line: 31
  category: scope
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "a change under website/ still runs the documentation check and skips the code jobs is promised and no acceptance claims it"
- file: .engineering/planning/story/planning-only-prs-skip-tests.md
  line: 32
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "one path list, in the workflow is promised and no acceptance claims a single list"
- file: .engineering/planning/story/test-step-within-budget.md
  line: 84
  category: scope
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "fails when it runs over 120 s, locally and in CI is promised and no acceptance has an overrunning step that fails"
- file: .engineering/planning/story/test-step-within-budget.md
  line: 76
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the adversary_sampling memory_bytes == 0 race is listed and no acceptance claims its fix"
- file: .engineering/planning/story/decisions-rules-on-input-predicates.md
  line: 58
  category: scope
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the reset-keeps-the-decisions-file outcome and its tests are not traceable to Origin"
- file: .engineering/planning/story/decisions-rules-on-input-predicates.md
  line: 35
  category: scope
  severity: warning
  verdict: needs-revision
  origin: pre-existing
  message: "first matching rule per request is promised and no acceptance tests precedence between two overlapping rules"
```

---
format: aep.planning-md/3
id: review-result:wave25-scope-r2
kind: review-result
status: active
title: Plan critic (scope), round 2, on wave 25
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

Approve: planning-only-prs-skip-tests, wasm-lanes-within-budget, guard-over-absent-optional-input,
identity-mint-types-and-fallback, reset-reapplies-seed, world-row-integer-fields. Needs-revision:
test-step-within-budget, decisions-rules-on-input-predicates. Every round-1 finding is fixed.

```findings
- file: .engineering/planning/story/test-step-within-budget.md
  line: 76
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Origin names only the slow tests review-017 reported; the Outcome and acceptance also claim the adversary_sampling race (PR 58) and the concurrent_reads failure (PR 60)"
- file: .engineering/planning/story/decisions-rules-on-input-predicates.md
  line: 30
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome promises 'An absent optional input matches no entry' and no acceptance claims it"
- file: .engineering/planning/story/decisions-rules-on-input-predicates.md
  line: 25
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome promises the world-JSON form under decisions.rules and no acceptance claims it"
- file: .engineering/planning/story/decisions-rules-on-input-predicates.md
  line: 44
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "an acceptance requires a load-time refusal the Outcome does not promise"
```

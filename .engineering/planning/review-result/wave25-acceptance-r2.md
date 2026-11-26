---
format: aep.planning-md/3
id: review-result:wave25-acceptance-r2
kind: review-result
status: active
title: Plan critic (acceptance), round 2, on wave 25
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

Approve: decisions-rules-on-input-predicates, guard-over-absent-optional-input,
identity-mint-types-and-fallback, reset-reapplies-seed, world-row-integer-fields. Needs-revision:
planning-only-prs-skip-tests, test-step-within-budget, wasm-lanes-within-budget.

```findings
- file: .engineering/planning/story/planning-only-prs-skip-tests.md
  line: 50
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "two bullets join the ci.yml jobs and the pages.yml job in one sentence; split each into one statement per workflow"
- file: .engineering/planning/story/test-step-within-budget.md
  line: 110
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "'assert no duration under 1 s' already holds for three of the six tests; name the three with a sub-second bound today (transport 5 ms, metrics_read 200 ms, concurrent_reads 100 ms) and what each becomes"
- file: .engineering/planning/story/wasm-lanes-within-budget.md
  line: 52
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Node lane's --test-timeout=60000 already holds (Taskfile.yml:83); state the Node lane is unchanged and the three wasm-bindgen lanes change from 120 to 60 or below"
```

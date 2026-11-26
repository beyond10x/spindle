---
format: aep.planning-md/3
id: review-result:wave25-design-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on wave 25
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

Approve: `story:wasm-lanes-within-budget`, `story:world-row-integer-fields`. Needs-revision: the
other six. No `depends_on` edge and no cycle among the eight.

```findings
- file: .engineering/planning/story/test-step-within-budget.md
  line: 84
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "'fails when it runs over 120 s' names no mechanism while story:wasm-lanes-within-budget plans a scripts/ timer for the same Taskfile check task; name one owner and give the other a depends_on edge"
- file: .engineering/planning/story/planning-only-prs-skip-tests.md
  line: 32
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "one path list in the workflow, but ci.yml and pages.yml both need it; say one script holds the list and both workflows call it"
- file: .engineering/planning/story/decisions-rules-on-input-predicates.md
  line: 76
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the ordered rule form is open: no file syntax, operators, default meaning or precedence against plain answers; flatten rejects arrays today"
- file: .engineering/planning/story/guard-over-absent-optional-input.md
  line: 29
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the outcome is 'as ESS answers it, or with a declared outcome' and does not pick one"
- file: .engineering/planning/story/identity-mint-types-and-fallback.md
  line: 62
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the body does not choose between widening mint::identity and widening the shared minted, and the types are not enumerated"
- file: .engineering/planning/story/reset-reapplies-seed.md
  line: 96
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the reset-waits-for-seed hook is open between the CLI and ingress; pick the CLI route and say how count_reset is kept"
```

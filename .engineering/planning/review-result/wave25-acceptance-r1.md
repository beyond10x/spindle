---
format: aep.planning-md/3
id: review-result:wave25-acceptance-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on wave 25
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

Approve: `story:world-row-integer-fields`, `story:reset-reapplies-seed`. Needs-revision: the other six.

```findings
- file: .engineering/planning/story/planning-only-prs-skip-tests.md
  line: 42
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "one bullet joins Gate/macOS within 2 minutes with Build documentation skipping the site build, and no bullet states that every required check reports success on a planning-only PR"
- file: .engineering/planning/story/test-step-within-budget.md
  line: 91
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the 120 s bound does not say whether compile time counts, 'on an idle runner' is not repeatable, and 'fails when it runs over 120 s' has no acceptance bullet"
- file: .engineering/planning/story/test-step-within-budget.md
  line: 93
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "'No test in the list above' refers to unnamed tests, and 'depends on wall-clock timing under 1 s' has no check anyone can run"
- file: .engineering/planning/story/wasm-lanes-within-budget.md
  line: 45
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the bullet for the four other lanes drops the empty target/ condition, and the 120 s checks name no pool load"
- file: .engineering/planning/story/decisions-rules-on-input-predicates.md
  line: 51
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "a_reset_keeps_the_decisions_file already holds: served_decider_survives_a_console_reset (crates/spindle-cli/tests/spindle-cli/adversary_served_decisions.rs:285)"
- file: .engineering/planning/story/decisions-rules-on-input-predicates.md
  line: 48
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "no bullet sends a request two rules both match, so a last-match implementation passes"
- file: .engineering/planning/story/guard-over-absent-optional-input.md
  line: 34
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "no state before the work and no expected outcome; if the interpreter answers undecidable the bullet passes on day one, and the interpreter-to-HTTP mapping is not named"
- file: .engineering/planning/story/identity-mint-types-and-fallback.md
  line: 42
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "'every identity type ESS mints' is not enumerated and the String-newtype spelling is not settled"
- file: .engineering/planning/story/identity-mint-types-and-fallback.md
  line: 44
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the three fallback forms (else literal, {input: other}, {generated: true}) are not named"
```

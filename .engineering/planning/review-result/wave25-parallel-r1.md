---
format: aep.planning-md/3
id: review-result:wave25-parallel-r1
kind: review-result
status: active
title: Plan critic (parallel-safety), round 1, on wave 25
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

All five findings are warnings. Every collision is a different hunk or file region; the plan does
not name the shared files. All 8 stories are placed by cited paths; 0 by inference alone; 0
unplaceable.

```findings
- file: .engineering/planning/story/guard-over-absent-optional-input.md
  line: 48
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "this story edits crates/spindle-kernel/src/execute.rs (:732-737, :846) and story:identity-mint-types-and-fallback lists the same file (:1903, inferred); neither names the other"
- file: .engineering/planning/story/decisions-rules-on-input-predicates.md
  line: 69
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "this story lists crates/spindle-cli/src/cli.rs help text and story:reset-reapplies-seed edits the same file at :311; neither names the other"
- file: .engineering/planning/story/test-step-within-budget.md
  line: 122
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "this story may edit crates/spindle-docs/src/status.rs and generated status pages on a test rename, and story:reset-reapplies-seed edits the same files; neither names the other"
- file: .engineering/planning/story/decisions-rules-on-input-predicates.md
  line: 70
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "this story's Node reset test lands in crates/spindle-node/tests/node/, whose exchanges.test.mjs replays the world-exchanges.json story:world-row-integer-fields re-records; neither names the other"
- file: .engineering/planning/story/wasm-lanes-within-budget.md
  line: 68
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "this story, story:planning-only-prs-skip-tests and story:test-step-within-budget all edit Taskfile.yml task check and the ci.yml Gate job, and no depends_on edge records the order"
```

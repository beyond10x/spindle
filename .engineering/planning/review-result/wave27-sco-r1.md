---
format: aep.planning-md/3
id: review-result:wave27-sco-r1
kind: review-result
status: active
title: Plan critic (scope), round 1, on wave 27
relations:
- reviews: story:stdin-run-fires-bindings
- reviews: story:stdin-arrange-and-line-references
- reviews: story:seed-once-on-durable-state
- reviews: story:serve-loop-signal-exit-answers
- reviews: story:query-input-beside-body
- reviews: story:caller-attributes-on-loopback
- reviews: story:map-input-guard-facts
- reviews: story:integer-identity-any-spelling
- reviews: story:wasm-lanes-within-budget
- reviews: story:refused-bound-command-recorded
- reviews: story:cross-component-related-reads
revision: 1
---
needs-revision

- story:cross-component-related-reads — the "Not covered" section still says reading another component's store in one process is left for later, but "Decided when scoped" D1 A and the "Joined" section deliver exactly that, so the stale exclusion contradicts the decided scope — .engineering/planning/story/cross-component-related-reads.md:90
- story:cross-component-related-reads — D2 A is cut mid-sentence at "refused by name in", so the stored text does not say that the refusal applies in every run, nor that nothing refuses it today. That is what makes D2 a promise separate from D1's one-host read — .engineering/planning/story/cross-component-related-reads.md:191
- story:serve-loop-signal-exit-answers — D1 A is cut at "acceptance 2". The scoped decision `decisions.md` records says acceptance 2 now reads "400 or 503, never EOF". The stored body keeps the old 400-only acceptance and loses the changed promise — .engineering/planning/story/serve-loop-signal-exit-answers.md:78
- story:stdin-arrange-and-line-references — D1 B is cut at "into the". The target of the `{"$line": n, "pointer": ...}` reference (the earlier line's printed answer) is missing, so the stored text does not say what a reference reads — .engineering/planning/story/stdin-arrange-and-line-references.md:103
- story:map-input-guard-facts — D1 A is cut at "its values by". The decided binding rule (a Map binds `count` and its values by index in key order, as ESS 0.99.1) is not in the body — .engineering/planning/story/map-input-guard-facts.md:76
- story:integer-identity-any-spelling — D1 A is cut at "the". The decided move of the world's integer walk into the kernel is missing, and `Scope` and `Surfaces` rely on it (`world.rs`, `lib.rs` re-export) — .engineering/planning/story/integer-identity-any-spelling.md:73
- story:seed-once-on-durable-state — D2 A is cut at "applies". The accepted consequence (a SIGKILL between commit and marker applies one entry twice) is missing — .engineering/planning/story/seed-once-on-durable-state.md:111
- story:caller-attributes-on-loopback — D1 A is cut at "naming". The flags the stderr line names are missing, and the acceptance for that line depends on them — .engineering/planning/story/caller-attributes-on-loopback.md:103
- story:caller-attributes-on-loopback — the title and Outcome still promise "A loopback served run serves a command that reads its caller". The settled second branch only changes the refusal text. A loopback run without `--fixed-caller` or the trusted-caller pair still answers 501, which is the consumer's gap in `## Origin`. The first branch (trust the caller header on loopback) is declined in `Scope` with no line in "Decided when scoped" saying the gap stays — .engineering/planning/story/caller-attributes-on-loopback.md:26
- story:wasm-lanes-within-budget — the body names the host `the build node` in a public repository, and this wave adds the line; it is not on `main` — .engineering/planning/story/wasm-lanes-within-budget.md:94
- story:wasm-lanes-within-budget — D1 A is cut at "a", so the rule for a new worktree's first run (reported beside the slot story) is not in the body — .engineering/planning/story/wasm-lanes-within-budget.md:118

What I read: 11 stories (`aep plan artifact show` for each, plus the story files directly to confirm the cuts), `.engineering/waves/wave-27.md`, `wave-26.md`, `aep plan artifact graph`, and the neighbouring stories (`bindings-cross-components`, `cross-component-view-refused`, `kernel-caller-attributes`, `world-row-integer-fields`, `reset-reapplies-seed`, `run-fails-on-rejected-line`). No epic is the parent, so I took the promises from the scratch records. Those are `JOINED-OPEN.md` (wave 27 list) and `w27/decisions.md`, plus the operator's order and the consumer "Origin" sections in the stories. I extracted 10 promises and traced all 10 to a story:
- seed once
- stdin arrange and references
- stdin bindings
- wasm lanes with the slot session's three cache changes
- the three adversary defects (map, integer, serve loop)
- cross-component reads
- query beside body
- caller attributes
- refused-bound recording (a consumer's origin; it is not in `JOINED-OPEN.md`)

No gap, no two stories claiming one outcome, and no reach beyond the order.

What I could not establish:
- `w27/decisions.md` holds the full D-texts, and the stored stories cut them at a line break. I take that as the cause of the truncation findings above and did not check how the store wrote them.
- The ESS-semantics drafts under "Later" in `JOINED-OPEN.md` (for example `command-input-decimal-text`) are not taken by this wave. The wave page does not name them. I treated them as an omission the drafter named in scratch, not as a gap. Whether the page should name them is for the operator.
- The wave page says wasm-lanes was promised "implemented in wave 26" to the slot session (`joined-d36d12a5.md`), and it now sits in wave 27. That is a communication matter, not scope.
- The `## Origin` sections quote route templates from a consumer's specification (`/api/v1/kv`, `/api/v3/tasks/template/{template}`). I did not count them as a person, company, host or path.
- Out of my lane: whether the acceptances of `story:stdin-run-fires-bindings` (refused bound command exits 0, D4 failed bound command exits 1) and `story:cross-component-related-reads` (the old refuse-by-name acceptance against the delivered one-host read) are checkable.
- Out of my lane: the merge order and shared surfaces on the wave page.

```findings
- file: .engineering/planning/story/cross-component-related-reads.md
  line: 90
  category: scope
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the Not covered section still says reading another component's store in one process is left for later, but Decided when scoped D1 A and the Joined section deliver it, so the stale exclusion contradicts the decided scope"
- file: .engineering/planning/story/cross-component-related-reads.md
  line: 191
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D2 A is cut mid-sentence at 'refused by name in', so the body does not say the affects refusal applies in every run and that nothing refuses it today"
- file: .engineering/planning/story/serve-loop-signal-exit-answers.md
  line: 78
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D1 A is cut at 'acceptance 2'; the decided change of acceptance 2 to '400 or 503, never EOF' is missing from the body, which keeps the old 400-only promise"
- file: .engineering/planning/story/stdin-arrange-and-line-references.md
  line: 103
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D1 B is cut at 'into the', so the body does not say what the reference pointer reads (the earlier line's printed answer)"
- file: .engineering/planning/story/map-input-guard-facts.md
  line: 76
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D1 A is cut at 'its values by', so the decided Map binding rule (count plus values by index in key order, as ESS 0.99.1) is missing from the body"
- file: .engineering/planning/story/integer-identity-any-spelling.md
  line: 73
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D1 A is cut at 'the', so the decided move of the world's integer walk into the kernel, which Scope and the wave page's Surfaces rely on, is missing from the body"
- file: .engineering/planning/story/seed-once-on-durable-state.md
  line: 111
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D2 A is cut at 'applies', so the accepted consequence (a SIGKILL between commit and marker applies one entry twice) is missing from the body"
- file: .engineering/planning/story/caller-attributes-on-loopback.md
  line: 103
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D1 A is cut at 'naming', so the flags the stderr line must name are missing from the body"
- file: .engineering/planning/story/caller-attributes-on-loopback.md
  line: 26
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the title and Outcome promise a loopback run serves a caller-reading command, but the settled second branch only changes the refusal text and the consumer's route stays unserved without flags; the first branch is declined with no line in Decided when scoped saying so"
- file: .engineering/planning/story/wasm-lanes-within-budget.md
  line: 94
  category: scope
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the body names the host the build node in a public repository; replace it with a generic phrase such as the build node"
- file: .engineering/planning/story/wasm-lanes-within-budget.md
  line: 118
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D1 A is cut at 'a', so the rule for a new worktree's first run (reported beside the slot story) is missing from the body"
```

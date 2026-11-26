---
format: aep.planning-md/3
id: review-result:wave27-des-r2
kind: review-result
status: active
title: Plan critic (design), round 2, on wave 27
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

- story:serve-loop-signal-exit-answers — its decided bullet says a cut reset "leaves `seed.json` at the count of entries done", but it does not say the marker is set to 0 when the reset empties the state, so a SIGTERM before the reset's first entry completes leaves the old full count over an empty `--state`, and a restart then skips the seed; the body (or `story:seed-once-on-durable-state`'s `step` decision) must state who writes count 0 at the reset and add an acceptance for the cut before the first entry — `.engineering/planning/story/serve-loop-signal-exit-answers.md:423-425`, `.engineering/planning/story/seed-once-on-durable-state.md:319-321`

**Round-1 finding:** resolved. `story:serve-loop-signal-exit-answers` now has `depends_on story:seed-once-on-durable-state`, and its Decided section states the marker behaviour after a cut reset (lines 423-425). The new finding above is the remaining gap in that same statement.

**What I read:**
- 11 story bodies, the wave page, and `review-result:wave27-des-r1`.
- Commands: `aep plan artifact show` for each story, `relations`, `graph`, `validate` (reports `valid`), and `git log` and `git show` on three story files.
- I walked every declared edge touching the 11 ids, including edges outside the set. The edges are `depends_on` from `story:serve-loop-signal-exit-answers` to `story:seed-once-on-durable-state`, `depends_on` from `story:wasm-lanes-within-budget` to `story:test-step-within-budget` (implemented), and `informed_by` edges to earlier stories. I found no cycle and no chain.
- Acceptance reads traced to a producer inside the set: 1 (the serve-loop reset test reads seed-once's marker), and 1 edge records it. The other acceptances read tree state or fixtures their own story creates.
- The other ordered pairs share files without one reading the other's output, so they are the shared-file trade-off. They are ordered on the wave page and need no edge:
  - U1: `story:stdin-run-fires-bindings` and `story:stdin-arrange-and-line-references` share the stdin loop.
  - U3: `story:query-input-beside-body` and `story:caller-attributes-on-loopback` share `served` in `surface.rs`.
  - U5, U7, U8: shared hunks in `execute.rs`, `host.rs` and `lib.rs`.

**What I could not establish (out of my lane, none of it sets the verdict):**
- Relations: `story:query-input-beside-body`, `story:caller-attributes-on-loopback` and `story:refused-bound-command-recorded` have no `serves vision:spindle` edge in `aep plan artifact show`, although the brief says all serve it. `git show` of their first commits shows they never had one; the round-1 record's claim that every story has one is wrong for these three. `story:refused-bound-command-recorded` carries only `informed_by story:stdin-run-fires-bindings`. This belongs to scope or the validator.
- Acceptance and scope: `story:serve-loop-signal-exit-answers` names the same tests differently in Acceptance and Scope. Its Acceptance says "a refused reset ... the client reads the 400", while D1 says "400 or 503". `story:stdin-run-fires-bindings` asks for an HTTP comparison test but plans a fixture with no `reached_by: network` component.
- Fixtures, line citations and timings are not verified against the tree.

```findings
- file: .engineering/planning/story/serve-loop-signal-exit-answers.md
  line: 423
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its decided bullet says a cut reset leaves seed.json at the count of entries done but not that the reset sets the marker to 0 when it empties the state, so a signal before the reset's first entry completes leaves a full count over an empty state and a restart skips the seed; state who writes count 0 at the reset (this story or seed-once's step decision) and add an acceptance for a cut before the first entry"
```

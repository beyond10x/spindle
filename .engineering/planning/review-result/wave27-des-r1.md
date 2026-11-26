---
format: aep.planning-md/3
id: review-result:wave27-des-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on wave 27
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
Now edges among the set and the story bodies.
needs-revision

- story:serve-loop-signal-exit-answers — it changes `seeding.step`, `reset_seeded` and a seeded reset's answer. Story:seed-once-on-durable-state rewrites the same `Seeding::step` to write a marker after each entry and changes the reset path to match. Its acceptance (a reset cut by SIGTERM "leaves the state as it was", on a durable `--state`) therefore reads state whose shape the seed story defines. No `depends_on` edge says so; the order "seed-once, then serve-loop" exists only in the `## Units` table of the wave page. The fix is `depends_on story:seed-once-on-durable-state` on this story. The trade-off is to keep one unit and one implementor, so the shared file is the reason and no edge is needed. If you keep that, the body must still say what the marker holds after a reset cut mid-seed, because the seed story says nothing about it — `.engineering/planning/story/serve-loop-signal-exit-answers.md` (Decided D2, Scope `:433-443`), `.engineering/planning/story/seed-once-on-durable-state.md` (Scope `step :340-382`, "Would collide with")

Each of the other findings I checked came out as no finding:
- **Cycle:** none. The only edges among the 11 stories are `serves vision:spindle`, `informed_by` (refused-bound to stdin-run-fires-bindings, map-input to guard-over-absent-optional-input, integer-identity to world-row-integer-fields, serve-loop to reset-reapplies-seed), and `wasm-lanes` `depends_on` `test-step-within-budget` (implemented).
- **Chain:** none. The units run in parallel; within U1, U2 and U3 the order is a shared-loop order and the pairs are separate features.
- **Split abstraction:** none. Each story demonstrates its own outcome end to end. This includes refused-bound, which carries spec, generated code, host, channel, ingress and UI in one vertical story.
- **U1 order:** the order is likewise only on the wave page. Story:stdin-arrange-and-line-references does not read anything that story:stdin-run-fires-bindings produces, so it is a shared-file trade-off, not a finding.
- **Cross-unit reads:** story:cross-component-related-reads reads nothing the other units produce in its acceptance. Its overlaps with U1, U3, U5 and U7 are shared files, ordered on the wave page, which is the parallel-safety lane.

What I read: 11 story bodies, the wave page, and the raw `## Decided when scoped` sections (for the truncation seen in the first reading). I also read `story:test-step-within-budget` and `story:planning-only-prs-skip-tests`.
- Commands: `aep plan artifact show` for each story, `relations`, `graph`, `validate` (reports `valid`) and `waves`.
- I walked every edge touching the 11 ids, including edges to artifacts outside the set (`vision:spindle`, the earlier stories, review and approval records), and found no cycle.
- Acceptance reads traced to a producer inside the set: 1 (serve-loop's `--state` reset test to seed-once's marker), with 0 recorded by an edge. The other acceptances read existing tree state or fixtures their own story creates.

What I could not establish, out of my lane (none of these set my verdict):
- **Truncated decisions:** the `## Decided when scoped` lines end mid-sentence in the story files themselves. They are:
  - stdin-arrange D1 B, the reference grammar ("... into the")
  - seed-once D2
  - caller-attributes D1
  - map-input D1
  - integer-identity D1
  - cross-component D2
  - wasm D1

  These decisions override Outcome and Acceptance, so they cannot be read as taken. That is the acceptance and scope lanes' concern.
- **Acceptance not updated:** story:cross-component-related-reads still lists refuse-by-name acceptances while the decided branch delivers reads from the owner's store. Story:caller-attributes-on-loopback has a title and Outcome that say "serves", and the decided branch only adds a stderr line.
- **Existing story in the set:** story:wasm-lanes-within-budget is an existing wave 25 story (status active, with a build-tooling scope), not a feature. Whether it belongs in "resolve all missing spindle features" is a scope-critic question.
- **Public repository:** I did not check the story bodies for people, hosts or home paths. I noticed "the build node" in the wasm story and a "slot session", which a scope or acceptance critic should look at.
- **Fixtures:** I did not verify fixtures, line citations or timings against the tree.

```findings
- file: .engineering/planning/story/serve-loop-signal-exit-answers.md
  line: 38
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its acceptance (a reset cut by SIGTERM leaves the durable state as it was) and its changes to seeding.step and reset_seeded read the seed marker that story:seed-once-on-durable-state writes in Seeding::step, and no depends_on edge runs from it to that story; add the edge, or keep both in one unit and state what the marker holds after a reset cut mid-seed"
```

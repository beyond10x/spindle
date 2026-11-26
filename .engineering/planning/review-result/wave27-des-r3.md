---
format: aep.planning-md/3
id: review-result:wave27-des-r3
kind: review-result
status: active
title: Plan critic (design), round 3, on wave 27
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
approve

What I read: 11 story bodies and the wave page in `<local path>`, plus `review-result:wave27-des-r1` and `-r2`. I ran `aep plan artifact show` on each story, plus `relations`, `graph` and `validate` (reports `valid`).

- **Earlier findings:** both are resolved.
  - Round 1 (the order of the seed story and the serve-loop story) is closed by `depends_on story:seed-once-on-durable-story` on `story:serve-loop-signal-exit-answers`. The edge is still in the graph.
  - Round 2 (who writes count 0 at a reset) is closed in two places. `.engineering/planning/story/seed-once-on-durable-state.md:113` says a reset with no world empties the state and writes `seed.json` at count 0 before its first entry. `.engineering/planning/story/serve-loop-signal-exit-answers.md:65` adds `a_signal_before_a_resets_first_entry_seeds_again_on_restart`.
- **Edges:** I walked every edge touching the 11 ids, including edges to artifacts outside the set (`vision:spindle`, earlier stories, review and approval records).
  - The edges are `serves vision:spindle` on all 11, and `depends_on` from serve-loop to seed-once.
  - Wasm-lanes has `depends_on story:test-step-within-budget`, which is implemented.
  - There are `informed_by` edges from serve-loop, map-input, integer-identity and refused-bound to earlier stories.
  - There is no cycle and no chain.
- **Acceptance reads:** I traced every acceptance line. One reads state a sibling in the set produces: the serve-loop acceptance on the cut reset reads seed-once's marker. One edge records it. Every other acceptance reads existing tree state or a fixture its own story creates.
- **Split abstractions:** none found. The pairs within U1, U2 and U3 are separate features that share a loop or function, and the wave page orders them. That is the shared-file trade-off, not a defect. The cross-unit overlaps (`execute.rs`, `host.rs`, `lib.rs`, `surface.rs`, `main.rs`, `serve.rs`) are shared hunks ordered in the wave page's Surfaces section.

What I could not establish (none of it sets the verdict):
- **Out of lane, scope:** `story:cross-component-related-reads` has a gap between its "Joined" paragraph and its decisions.
  - The consumer's refusal came from a stdin or served run.
  - The decisions keep `crates/spindle-cli/src/main.rs:882` (stdin `answer`, `execute_as`) unchanged, and name no setting for the stdin run's `unsupported_outcomes(…, None)` call at `:722`.
  - `crates/spindle-kernel/src/supported.rs` `foreign` (`:238`) refuses a foreign read from IR ownership alone, so a stdin run would likely still refuse.
- **Out of lane, acceptance:** `story:seed-once-on-durable-state` lists no test of its own for the count-0 reset rule (`seed-once-on-durable-state.md:113`). Only the serve-loop acceptance covers it.
- **Out of lane, cosmetic:** the H1 of `story:caller-attributes-on-loopback` says "serves", while its decided outcome only prints a stderr line.
- **Out of lane, public-repo check:** the wasm story body says "the build node" and "the workstation pool". I found no person, company, host name or home path in the 11 bodies or the wave page by grep.
- **Not verified:** line citations, fixtures and timings against the tree.

```findings
[]
```

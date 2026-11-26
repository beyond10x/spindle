---
format: aep.planning-md/3
id: review-result:wave27-par-r1
kind: review-result
status: active
title: Plan critic (parallel-safety), round 1, on wave 27
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

- story:integer-identity-any-spelling — this story and story:cross-component-related-reads both land on `crates/spindle-kernel/src/lib.rs` (inferred: both list it as "also likely", this story at `:104` beside `admits`, the other at `:95` for the `takes`/`runs` re-export), and neither "Would collide with" nor the wave page's Surfaces section names the file, so the coordinator has no entry for it at merge. Name it with one of two remedies, without choosing: an ordering edge with the file as its reason (the page's U5-before-U8 order already exists for `execute.rs`), or keep the new export out of `lib.rs`. — .engineering/planning/story/integer-identity-any-spelling.md:62, .engineering/planning/story/cross-component-related-reads.md:136, .engineering/waves/wave-27.md:36
- story:cross-component-related-reads — its Scope lists callers only in `serve.rs`, `component.rs` and `main.rs`, but D1 A widens `takes`, `check`, `execute_refusing` and the surface constructors. Their other callers are not in its scope or on the wave page (inferred: it does not say whether the widening is additive).
  - `Surface::new` or `Surface::fixing_callers` is called from `crates/spindle-control/src/control.rs:166`, `crates/spindle-control/src/bin/spindle-control-fixture.rs:99`, `crates/spindle-embed/src/lib.rs:174` and about 20 files under `crates/spindle-cli/tests/spindle-cli/`. One of them is `caller.rs`, which story:caller-attributes-on-loopback edits.
  - `spindle_kernel::runs`, `takes` or `selects` is called from about 15 files under `crates/spindle-kernel/tests/spindle-kernel/`.
  - `execute_as` is called at `crates/spindle-cli/src/main.rs:882`, inside the stdin `answer` that story:stdin-run-fires-bindings rewrites, and its new dispatch adds more calls.
  - Fix: say the widening is additive (a new function or a defaulted set) so these callers are untouched, or list them in the Scope and in the page's Surfaces section with the merge order that already puts U8 last. — .engineering/planning/story/cross-component-related-reads.md:136-138, .engineering/planning/story/cross-component-related-reads.md:198, `git grep -n "Surface::\(new\|proving_callers\|fixing_callers\)" -- crates`

What I read: all 11 stories (`aep plan artifact show` on each, plus the raw "Decided when scoped" sections in `.engineering/planning/story/` where the show output truncated them), the wave page, and `aep plan artifact graph`, `waves` and `relations`. I checked the stories they cite (`run-fails-on-rejected-line`, `test-step-within-budget`, `reset-reapplies-seed`, `world-row-integer-fields`) and grepped the worktree for callers.

Surfaces: cited 11, inferred-only 0, unplaceable 0. Every story's primary surface is cited in its body, and several secondary surfaces (new test modules, generated docs) are inferred.

Acceptance reads traced to a producer inside the set: 2, with 0 recorded by a `depends_on` edge.
- The arrange story's references read the printed answer that story:stdin-run-fires-bindings changes.
- The signal-exit story's reset-behind-seed test reads the seed marker and `reset_seeded` that story:seed-once-on-durable-state changes.
- Both pairs sit in one unit, built in the stated order by one implementor, so I did not file them.
- The wasm story reads `scripts/within.sh` from story:test-step-within-budget. That story is outside the set, implemented, edged, and the file exists in the tree.

Could not establish:
- Whether story:integer-identity-any-spelling and story:cross-component-related-reads change the same lines of `lib.rs`. The two sites are 9 lines apart and I assumed they differ.
- The wave page says U5 "may" move the world's integer walk out of `world.rs`, but D1 A decides it does. story:stdin-arrange-and-line-references also lists `world.rs` as "only if". Both bodies already name it generically, so I did not file it, but the page lists only U5.
- story:map-input-guard-facts lists `answer.rs` in its scope though it changes nothing there. The page says so, so I did not file it.
- A 120 s timing acceptance for U6 is measured while seven other units build in the shared pool. The story records the `slot status` header, so I left it.
- Out of my lane: the wasm story's acceptance cites `Taskfile.yml:58/66/74` for `WASM_BINDGEN_TEST_TIMEOUT` while its Scope cites `:100/108/116` for the runner; the stdin-bindings acceptance "run exits 0" on a refused bound command conflicts with its D4 "run exits 1"; and story:cross-component-related-reads under-scopes its test files. These stay with the acceptance, design and scope critics.
- Public repository: I flagged no person, company, host or home path in the bodies. I did not run a text search for them.

```findings
- file: .engineering/planning/story/integer-identity-any-spelling.md
  line: 62
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "this story and story:cross-component-related-reads both land on crates/spindle-kernel/src/lib.rs (inferred: both list it as also-likely, at :104 and :95), and neither body's collision list nor the wave page's Surfaces section names the file; name it with an ordering edge or keep the new export out of lib.rs"
- file: .engineering/planning/story/cross-component-related-reads.md
  line: 198
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D1 A widens takes, check, execute_refusing and the surface constructors, whose other callers (spindle-control, spindle-embed, about 20 cli test files including caller.rs that story:caller-attributes-on-loopback edits, about 15 kernel test files, and main.rs:882 in the stdin answer that story:stdin-run-fires-bindings rewrites) are in neither this scope nor the wave page (inferred: the body does not say the widening is additive); state that it is additive or list the callers"
```

---
format: aep.planning-md/3
id: review-result:wave27-par-r2
kind: review-result
status: active
title: Plan critic (parallel-safety), round 2, on wave 27
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

Both round-1 findings in my lane are resolved. I found nothing new that would change the plan. I raise no finding for the pairs inside one unit.

- **Round-1 finding 1 (`lib.rs`).** `.engineering/waves/wave-27.md:36` now lists `crates/spindle-kernel/src/lib.rs` under Surfaces. It gives U5's re-export beside `admits` (`:104`) and U8's beside `runs` (`:95`), with U5 merging before U8. `cross-component-related-reads.md:208` says the same.
- **Round-1 finding 2 (widening callers).** `cross-component-related-reads.md:203-207` and the page now say the widening is additive. `takes`, `check`, `execute_refusing`, `execute_as` and the `Surface` constructors keep their signatures. New functions carry the reached set. The callers in `spindle-control`, `spindle-embed`, the kernel and CLI test files, and `main.rs:882` stay unchanged.
- **Merge order.** The page's order (U6, U4, U5, U2, U1, U3, U7, U8) satisfies every shared-file constraint the page states. On `execute.rs` and `lib.rs` U5 precedes U8. On `host.rs` U7 precedes U8. On `surface.rs` U3 precedes U8. On `main.rs` U1 precedes U8. On `serve.rs` U2, U3 and U8 are in that order.
- **Shared files outside those two.** The other shared files are all named on the page: `cli.rs`, `status.rs`, `answer.rs`, the test `main.rs` module lists, `AGENTS.md`, `README.md`, `CHANGELOG.md` and the design doc § 11.
- **`main.rs` for U2 and U3.** `serve-loop-signal-exit-answers` and `caller-attributes-on-loopback` list `main.rs` only as a read site (`SIGNALLED` at `:436`, the exposed-run check at `:502`). They edit nothing there, so the page's U1/U8-only entry for it stands.
- **No edge for the arrange reference.** `story:stdin-arrange-and-line-references` references the printed answer that `story:stdin-run-fires-bindings` changes, but no `depends_on` edge records it. Both stories are in U1, built in order by one implementor, so I did not file it.

**What I read:**
- All 11 story bodies in `.engineering/planning/story/`.
- `.engineering/waves/wave-27.md`.
- `review-result:wave27-par-r1`, via `aep plan artifact show`.
- Greps of the worktree for `Router`, `RETRY_ATTEMPTS`, `attempt` and `invoke`.

**Surfaces:**
- Cited: 11 of 11.
- Inferred only: 0.
- Unplaceable: 0.

**Acceptance reads traced to a producer in the set:** 2.
- The serve-loop story reads the seed marker that `story:seed-once-on-durable-state` writes. A `depends_on` edge records it (`serve-loop-signal-exit-answers.md:10`).
- The arrange story reads the answer that `story:stdin-run-fires-bindings` prints. No edge records it, and both are in one unit.

So 1 of 2 is recorded by an edge.

**Could not establish:**
- Whether `story:stdin-run-fires-bindings` stays off `spindle-host` and `spindle-channel`. Its "Not touched" line is inferred. Its symbols are the channel `Router` and `Route::invoke`, and the retry-once acceptance concerns `RETRY_ATTEMPTS`, whose retry logic sits in `attempt` in `host.rs`. If its implementor reuses `Host`, it would land on the hunks of `story:refused-bound-command-recorded` and `story:cross-component-related-reads`, which the page's `host.rs` entry does not cover.
- The page's `world.rs` entry names U5 only. `story:stdin-arrange-and-line-references` lists `world.rs` as "only if" it exposes a per-row reader. Its collision line already names the file, so I did not file it.
- Out of my lane, noted in round 1 and not rechecked:
  - The wasm story's `Taskfile.yml` line numbers differ between its acceptance and its Scope.
  - The `run exits 0` acceptance in `story:stdin-run-fires-bindings` against its D4 "exits 1".
  - Under-scoped test files in `story:cross-component-related-reads`.
- I did not run a text search for personal names, hosts or home paths in the public bodies.

```findings
[]
```

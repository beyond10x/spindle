---
format: aep.planning-md/3
id: review-result:wave34-par-r1
kind: review-result
status: active
title: Plan critic (parallel-safety), round 1, on wave 34
relations:
- reviews: story:release-artifacts-signed
- reviews: story:unknown-commit-answered-as-unknown
- reviews: story:reset-on-a-busy-store-keeps-the-store
revision: 1
---
needs-revision

story:reset-on-a-busy-store-keeps-the-store — its acceptance `a_reset_whose_store_fails_otherwise_still_loses_it` builds a removal that fails with `StoreError::Unknown`, a variant that story:unknown-commit-answered-as-unknown adds in `crates/spindle-kernel/src/store.rs` (not yet in the tree), and no `depends_on` edge runs from this story to that one, so one wave would hold both. Add `depends_on story:unknown-commit-answered-as-unknown` to this story, or merge the two. The wave page says the opposite ("No `depends_on` edge inside the wave") and "Order and parallel safety" has U3's implementor read the variant from U2's branch "before its last round". On its own branch U3 cannot compile that case, so it ends up with a stand-in variant or a red build until U2 lands. If the edge is added, `aep plan artifact waves` may no longer place the two in one wave; that is the operator's to weigh. — `.engineering/planning/story/reset-on-a-busy-store-keeps-the-store.md:93` (acceptance), `.engineering/planning/story/unknown-commit-answered-as-unknown.md` (Outcome: "reports it as a store error of its own, `StoreError::Unknown`"), `.engineering/waves/wave-34.md:66`

What I read: 3 stories, the wave page, and the repository's `busy_store.rs` and `main.rs`. Commands: `aep plan artifact show` for each id, `aep plan artifact graph` and `relations`, `git grep`, `sed`.

Surfaces placed:
- **Cited:** 3 stories (U1 on `release.yml`; U2 on `answer.rs`, `sqlite.rs` and `capabilities.md`; U3 on `host.rs`, `console.rs` and `adversary_w33_u2.rs`).
- **Inferred:** 0 stories.
- **Unplaceable:** 0 stories.
- **Acceptance reads traced to a producer in the set:** 1, `StoreError::Unknown`, read by U3 and produced by U2. `depends_on` records 0 of those 1.

Collisions checked and found named, so not findings:
- **Shared files:** `CHANGELOG.md`, the `capabilities.md` paragraph, `environment-and-admission.md`, `answer.rs`, `store.rs` and `main.rs`. The wave page's Surfaces section names each with the merge order, and U2's and U3's "Would collide with" lines name each other.
- **Test stubs:** `busy_store.rs` stubs are private to that file, so U2's "built like `busy_store.rs`" copies and does not edit it.
- **Shared read-only citations:** U3's `serve.rs` edit is comment-only, and U2 only reads `serve.rs:814`.
- **U1:** shares only `CHANGELOG.md` with the other two. Its two permissioned jobs run on no pull request and a stub case checks that, which fits the brief.
- **Repository hygiene:** no person, organisation, host or home path in a story body. The repository's Python rule is not touched.

What I could not establish:
- Whether `aep plan artifact waves` places the two stories in separate waves once the edge exists. I did not run it, because it is the operator's to weigh.
- The Python rule, the repository's public-content rule and the test-time budgets are outside my lane. I saw nothing to flag on them and they did not set the verdict.

```findings
- file: .engineering/planning/story/reset-on-a-busy-store-keeps-the-store.md
  line: 93
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "its acceptance a_reset_whose_store_fails_otherwise_still_loses_it reads StoreError::Unknown, which story:unknown-commit-answered-as-unknown creates in crates/spindle-kernel/src/store.rs, and no depends_on edge runs from this story to that one (wave-34.md:66 says none exists), so one wave would hold both and U3 cannot compile that case on its own branch; add depends_on story:unknown-commit-answered-as-unknown or merge the two"
```

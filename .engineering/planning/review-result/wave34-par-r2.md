---
format: aep.planning-md/3
id: review-result:wave34-par-r2
kind: review-result
status: active
title: Plan critic (parallel-safety), round 2, on wave 34
relations:
- reviews: story:release-artifacts-signed
- reviews: story:unknown-commit-answered-as-unknown
- reviews: story:reset-on-a-busy-store-keeps-the-store
revision: 1
---
approve

Round 1's one finding is resolved and the revision broke nothing I could find.

- **Round 1 finding, resolved.** U3's `a_reset_whose_store_fails_otherwise_still_loses_it` at `.engineering/planning/story/reset-on-a-busy-store-keeps-the-store.md:357-361` now fails the removal with `StoreError::Backend`, which exists today. U3's Outcome and decisions no longer name `StoreError::Unknown`. U2 owns `a_reset_whose_commit_is_unknown_still_loses_the_store`, since it adds the variant. The wave page (`.engineering/waves/wave-34.md`, "Revision round 1") no longer has U3 read U2's branch. No story's acceptance reads state another story in the set creates.
- **New U2/U3 overlap in `supplied.rs`, named and safe.** Both add cases to `crates/spindle-host/tests/spindle-host/supplied.rs`, which today holds only the `Shared` wrapper that cannot fail. The wave page's Surfaces section names the file, separates the cases by case and stub-store names, and keeps the merge order U1, U2, U3. Both stories' scope and "Would collide" lines say the same.
- **U3's `serve.rs` edit.** It is comment-only. U2 only reads `serve.rs:814`.
- **`busy_store.rs` and `main.rs`.** U2 copies the stub into the new `unknown_commit.rs` and never edits `busy_store.rs`. U3 puts its ingress case in `busy_store.rs`. Only U2 touches `main.rs`.
- **U1.** It shares only `CHANGELOG.md` with the other two. Nothing else touches `release.yml`, the release test script, `AGENTS.md`, `README.md` or `Taskfile.yml`.

**What I read:** 3 stories, the wave page and `review-result:wave34-par-r1`. I ran `aep plan artifact show` on each story and on the round-1 result, and `aep plan artifact graph`. I also read `supplied.rs`, `busy_store.rs`, the ingress `main.rs`, `StoreError` in `store.rs` and the round-1 diff in git.

- **Surfaces placed:** 3 cited, 0 inferred, 0 unplaceable.
- **Acceptance reads traced to a producer in the set:** 0 across stories, so `depends_on` records 0 of 0. The `IMAGES` file that U1's `sign` reads is produced by U1's own `image-manifest` job.

**What I could not establish:**
- U3 must classify store errors in `Host::empty` with a catch-all or `matches!`, not an exhaustive match, or U2's new variant breaks U3's build when U3 merges last. U2's story says no match on `StoreError` lists every variant today. I could not see U3's new code, so this is an unease and not a finding.
- The wave page says CHANGELOG bullets are "ordered by story id" but also merges U2 before U3, and by id `reset` sorts before `unknown`. It is a harmless ordering ambiguity in one file.
- Outside my lane, I saw nothing to flag on the permissions guard, the test budgets, Python, or person, host or path text in the story bodies.

```findings
[]
```

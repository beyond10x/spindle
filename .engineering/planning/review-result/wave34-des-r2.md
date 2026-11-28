---
format: aep.planning-md/3
id: review-result:wave34-des-r2
kind: review-result
status: active
title: Plan critic (design), round 2, on wave 34
relations:
- reviews: story:release-artifacts-signed
- reviews: story:unknown-commit-answered-as-unknown
- reviews: story:reset-on-a-busy-store-keeps-the-store
revision: 1
---
approve

I read all three stories, the wave page and the round-1 design review. Round 1's one blocker is resolved, and the revision broke nothing in my lane.

- **Round-1 finding resolved.** `story:reset-on-a-busy-store-keeps-the-store` no longer reads `StoreError::Unknown`. Its case `a_reset_whose_store_fails_otherwise_still_loses_it` now fails a removal with `StoreError::Backend`, and its Outcome and decisions name no `Unknown`. The only remaining mentions are descriptive: a Scope "Would collide with" line, which says the case for `Unknown` belongs to U2, and a Safety fact line saying `UnknownCommit` does not map to `Busy`. Neither is an acceptance read. U2 owns `a_reset_whose_commit_is_unknown_still_loses_the_store`, which is a natural fit because U2 adds the variant.
- **No new hidden dependency.** Both U2's and U3's host cases in `supplied.rs` run against the host as it is today. U2's case passes before and after U3, because every non-`Busy` error still loses the store. Each unit writes its own stub store in `supplied.rs` and `busy_store.rs`. The wave page's merge order (U1, U2, U3) is recorded as an order for shared files and the same `capabilities.md` paragraph. It is not a chain, since all three build from `wave/34` in parallel. The shared files are a parallel-safety question and are recorded.
- **U1 is a single story.** The `sign`, `image-manifest` and `publish` split follows job boundaries inside one workflow. The new stub case `adversary_backfill_verifies_at_main` and the named guard forms in `adversary_id_token_only_on_signing_jobs` match the tree: `release.yml:691` is `${{ github.event_name != 'pull_request' && …` and `:766` is `${{ !cancelled() && github.event_name != 'pull_request' && …`.
- **Public repository.** I found no person, host or home path in the three bodies or the wave page.

**What I read:** 3 stories, the wave page, `review-result:wave34-des-r1`, `release.yml` lines 28-40 and 686-772, and the two test files' headers. Commands: `aep plan artifact show` on each story, `relations`, `graph`, `validate` (reports `valid`, with only stale review-result notes that are not mine) and `waves`.

- **Edges:** I walked every edge touching the set (`serves`, `decomposes`, `informed_by`), including to `vision:spindle`, `epic:production-service` and the two stories outside the set. There are no `depends_on` edges and no cycle.
- **Acceptance reads:** I traced 11 acceptance reads. None reads state a sibling produces, so no edge is owed.

**What I could not establish:**
- Whether U2's `unknown_commit.rs` stub store is a copy of the one in `busy_store.rs` or imports it. `busy_store.rs` defines its `Busy` stub privately, so a copy is the likely reading. If it imports, that is a parallel-safety matter for U3's edits to that file, and out of my lane.
- Whether a `match` in `Host::empty` that is exhaustive over `StoreError` would break at the U2 merge. The U3 body says only `Busy` keeps the store, which reads as a `matches!`, so I left it.
- Collisions with stories outside the set on `host.rs` and `CHANGELOG.md` show in `aep plan artifact waves`. Those belong to the parallel-safety critic.

```findings
[]
```

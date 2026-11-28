---
format: aep.planning-md/3
id: review-result:wave34-des-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on wave 34
relations:
- reviews: story:release-artifacts-signed
- reviews: story:unknown-commit-answered-as-unknown
- reviews: story:reset-on-a-busy-store-keeps-the-store
revision: 1
---
Now the three stories.
needs-revision

- `story:reset-on-a-busy-store-keeps-the-store` — its acceptance case `a_reset_whose_store_fails_otherwise_still_loses_it` builds a removal that fails `StoreError::Unknown`, a variant `story:unknown-commit-answered-as-unknown` creates, and no `depends_on` edge runs from U3 to U2. The wave page says "No `depends_on` edge inside the wave" and handles the order only in prose (U3 reads U2's branch before its last round). The test does not compile on `wave/34` until U2 lands. Either add `depends_on story:unknown-commit-answered-as-unknown` to U3, or drop the `Unknown` half of the case and leave that classification to U2. `aep plan artifact graph` shows no edge between the two. — `.engineering/planning/story/reset-on-a-busy-store-keeps-the-store.md:93`, `.engineering/planning/story/unknown-commit-answered-as-unknown.md:33`

**What I read:** 3 stories, `.engineering/waves/wave-34.md` and the `release.yml` job guards. I ran `aep plan artifact show` on each story, plus `relations`, `graph`, `validate` and `waves`.
- **Edges and cycles:** I walked every edge touching the set (`serves`, `decomposes`, `informed_by`), including to `vision:spindle`, `epic:production-service` and the two archived or implemented stories. There are no `depends_on` edges and no cycle.
- **Acceptance reads:** I traced 9 reads to producers. One reads state a sibling produces (U3 reads U2's `StoreError::Unknown`), and no edge records it. The rest read the story's own output or existing tree.
- **Validate:** it reports `valid`, with only stale review-result notes that are not mine.
- **Public-repo check:** I found no person, host or home path in the three bodies, the draft story or the wave page.
- **Pull-request run:** `image-manifest` already carries the pull-request guard at `release.yml:691`, so the planned `id-token: write` on it holds.

**What I could not establish:**
- Whether `aep plan artifact waves` places U3 beside U2. It does, since no edge exists; that is the cost of the finding above.
- Out of my lane, for the acceptance critic: `adversary_id_token_only_on_signing_jobs` requires an `if:` that starts with `github.event_name != 'pull_request'`. The existing guards read `${{ github.event_name …` (`release.yml:691`) or start with `!cancelled() &&` (`:766`).
- Out of my lane, for the parallel-safety critic: U3's wave-table surface omits files its Scope lists (`serve.rs` comment, `rows.rs`), and the `waves` output shows collisions with stories outside this set on `host.rs` and `CHANGELOG.md`.

```findings
- file: .engineering/planning/story/reset-on-a-busy-store-keeps-the-store.md
  line: 93
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "its acceptance case a_reset_whose_store_fails_otherwise_still_loses_it fails a removal with StoreError::Unknown, a variant story:unknown-commit-answered-as-unknown adds (unknown-commit-answered-as-unknown.md:33), and no depends_on edge runs from this story to it; add depends_on story:unknown-commit-answered-as-unknown, or drop the Unknown half of the case"
```

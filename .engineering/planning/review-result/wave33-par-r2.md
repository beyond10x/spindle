---
format: aep.planning-md/3
id: review-result:wave33-par-r2
kind: review-result
status: active
title: Plan critic (parallel-safety), round 2, on wave 33
relations:
- reviews: story:listener-limits
- reviews: story:new-leader-write-while-old-paused
- reviews: story:seed-record-after-a-component-is-removed
- reviews: story:entity-invariant-over-stored-rows
revision: 1
---
approve

I found no collision the plan leaves unnamed, and the round-1 finding on `answer.rs` is fixed.

**Round-1 finding (`review-result:wave33-par-r1`).** The `answer.rs` collision between U1 and U2 is fixed. U2 now owns `crates/spindle-ingress/src/answer.rs`. U1 builds its queued-request `503` in `listener.rs` from `Answer::refusal`, and doesn't edit `answer.rs`. The story bodies and the wave page's Surfaces section both say so (`.engineering/waves/wave-33.md:120-123`). The tree supports this: `Answer`'s `status`, `body` and `headers` are public (`answer.rs:29,32,36`), and `refusal` is at `:42`.

**Shared files.** I compared all four Scope lists file by file. Ten files or sets are shared:
- `CHANGELOG.md`, `cli.rs` and `cli.md` (all four units)
- `AGENTS.md` (U1, U3)
- `capabilities.md` (U1, U2)
- the ingress tests' `main.rs` (U1, U2)
- `replicas.rs` (U2, U3)
- the kernel tests' `postgres.rs` (U2, U3)
- `environment-and-admission.md` (U2, U3, U4)
- `status.rs` with `status.md` and `status.json` (U3, U4)

Every one is in Surfaces, and so is `adversary_w31_u6.rs` against `story:postgres-lane-margin`. "No other file is in two units' scope" holds.

**Edit points in the tree:**
- **`cli.rs`:** U3's `--seed` help edit (the link at `:569`) and U1's new flags after `drain: u64` (`:594`) are about 25 lines apart, so they won't conflict.
- **`status.rs`:** U4's row is at `:145` and U3's at `:488`.

**Hidden compile collisions.** None:
- **U2's new `StoreError` variant:** nothing outside `answer.rs` matches it exhaustively. The only matches are `Conflict` arms in `establish.rs:52` and kernel tests. So no edit is forced in U3's `postgres.rs` or `database.rs`.
- **U4's new `Misfit` variant:** outside `fit.rs` it is used only in tests that compare values.

**What I read:** the 4 stories, the wave page and `review-result:wave33-par-r1` (`aep plan artifact show`, `aep plan artifact graph`), plus the tree at c31c23b7: `git diff --stat 276298bb HEAD`, `cli.rs:545-600`, `status.rs:140-150,484-490`, the `answer.rs` API, `replicas.rs:20-30`, the ingress test module list, and greps for `StoreError::` and `Misfit::` matches and for `postgres_prefix`.
- **Surfaces:** 4 of 4 units placed with cited surfaces, 0 inferred only, 0 unplaced.
- **State the acceptance tests read:** 0 traced to a producer in the set, so 0 `depends_on` edges are needed and 0 are recorded. I checked one candidate and it is not a dependency: U1's "existing lane tests pass with the 30 s default" against U2's two rewritten `adversary_w31_u6.rs` cases. Those cases wait at most 11 s either way.

**Other checks:**
- **PostgreSQL lane budget:** every new lane test states its time. U2's are under 15 s and 5 s. U3's eight are each under 10 s, about 80 s summed on the wave page. U4's one is under 5 s. U2's rewrite of `a_resumed_leader_posts_no_event_the_new_leader_committed` can add a retry of at most 10 s, inside its stated "under 30 s".
- **Public repository:** the stories, the wave page and the round-1 reviews name no person, organisation, host or home path.

**What I could not establish:**
- Whether `story:postgres-lane-margin` collides with U2 inside `adversary_w31_u6.rs`. That story is outside the set and records no scope. The plan handles it by merging the margin story first and having U2 rebase onto it.
- Out of my lane (acceptance or design critic): none of U3's eight lane tests has been timed. With U2 and U4, their stated bounds come to about 105 s against the lane's 100 s parallel bound, so the wave depends on the margin story landing first.

```findings
[]
```

---
format: aep.planning-md/3
id: review-result:wave33-par-r1
kind: review-result
status: active
title: Plan critic (parallel-safety), round 1, on wave 33
relations:
- reviews: story:listener-limits
- reviews: story:new-leader-write-while-old-paused
- reviews: story:seed-record-after-a-component-is-removed
- reviews: story:entity-invariant-over-stored-rows
revision: 1
---
needs-revision

story:listener-limits — U1's queued-request `503` with `{"refused", "committed": false}` needs the same response body as U2's store-deadline `503`, but U1's scope and the wave page's Surfaces section leave out `crates/spindle-ingress/src/answer.rs`. The only function that builds that body today is `Answer::unfinished`, which always returns `501` (`answer.rs:85-91`), and U2 changes that file. Both units may add a 503 form of it in the same place, or U2 may change its signature under U1. Two remedies, not chosen here: name `answer.rs` in U1's scope and in Surfaces with one unit owning the 503 builder, or add an ordering edge that gives `answer.rs` as its reason. This rests on an inferred surface for U1. — .engineering/planning/story/listener-limits.md:50, :100; .engineering/planning/story/new-leader-write-while-old-paused.md:136; .engineering/waves/wave-33.md:91

**What I read:** 4 stories (`aep plan artifact show` for each), `aep plan artifact graph` and `relations`, `wave-33.md`, and against the tree at fc19c7d9:
- `answer.rs` and `listener.rs`
- `StoreError` and `backend`, and every place that matches on them
- the `Misfit` call sites, and the CLI and kernel test module layout and schema isolation
- `Taskfile.yml` `check:postgres:run` and its filter
- eventlog 0.10.0 `publication_gate`, whose advisory key includes `current_schema()`

**Counts:**
- Surfaces: 4 of 4 items placed with cited surfaces (each also has inferred lines), 0 inferred only, 0 unplaced.
- Acceptance reads: 0 traced to a producer in the set, so 0 `depends_on` edges are needed and 0 are recorded.

**What I confirmed:**
- Every other shared file the scopes name is in the Surfaces list.
- U2's fix needs no change to `postgres.rs`, U3's file: the commit path maps errors through `sqlite.rs` `backend`.
- U4 needs no change to `database.rs` or `serve.rs`: `misfits` already gets the store, and misfits are only displayed outside `fit.rs`.
- Tests that run in parallel in the lane cannot block each other through U2's lock: kernel tests use fresh components (`fresh`) and CLI tests use fresh schemas (`schema()`).
- Every new test in the PostgreSQL lane states its expected time: U2 15 s and 5 s, U3 under 10 s each, U4 under 5 s. None adds more than about 10 s without saying so.
- No person, organisation, host or home path appears in any story body or in the wave page.

**What I could not establish:**
- Collision with `story:postgres-lane-margin`, which is outside the set. Its body names `adversary_w31_u6::postgres::a_healthy_leader_posts_promptly_on_a_short_and_a_long_lease`. U2 rewrites two other cases in the same file, `adversary_w31_u6.rs`. That story records no scope, so whether the two collide is not assessed. U2's "under 30 s" for `a_resumed_leader_posts_no_event_the_new_leader_committed` also sits at that story's 30 s ceiling.
- Out of my lane (scope critic): the "Seed at start" status row (`crates/spindle-docs/src/status.rs:488`) says "a store kept elsewhere records nothing". U3 does not list `status.rs`. If U3 edits it, it shares that file with U4, which edits the row at `:145`.

```findings
[
  {
    "file": ".engineering/planning/story/listener-limits.md",
    "line": 100,
    "category": "parallel-safety",
    "severity": "warning",
    "verdict": "needs-revision",
    "origin": "introduced",
    "message": "inferred surface: U1's queued-request 503 with {\"refused\", \"committed\": false} needs the same response body as U2's store-deadline 503, but U1's scope and the wave page's Surfaces section leave out crates/spindle-ingress/src/answer.rs, where the only function that builds that body (Answer::unfinished, answer.rs:85-91) always returns 501 and which U2 changes; both units may add a 503 form of it in the same place, or U2 may change its signature under U1; remedies: name answer.rs in U1's scope and in Surfaces with one unit owning the 503 builder, or add an ordering edge that gives answer.rs as its reason"
  }
]
```

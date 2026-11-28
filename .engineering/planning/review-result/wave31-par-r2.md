---
format: aep.planning-md/3
id: review-result:wave31-par-r2
kind: review-result
status: active
title: Plan critic (parallel-safety), round 2, on wave 31
relations:
- reviews: story:postgres-lane-kills-slow-tests
- reviews: story:spindle-image-published
- reviews: story:listener-tls
- reviews: story:verified-caller-token
- reviews: story:spec-change-over-stored-rows
- reviews: story:several-writers-one-postgres
- reviews: story:event-egress-webhook
- reviews: story:dead-letters-kept-and-replayed
revision: 1
---
approve

I read the revision (commit 278f3126) and the whole set again. All seven round-1 findings are fixed, and I found nothing new.

**Round-1 findings**
1. Fixed. `story:several-writers-one-postgres` now has `depends_on` edges to `story:event-egress-webhook` and `story:dead-letters-kept-and-replayed`. `aep plan artifact graph` shows both. The wave page records them.
2. Fixed. The Scope of `story:several-writers-one-postgres` now lists the `listener.rs` hunks in `answer` (`:609`) and in `bind_reading_on` and `serve` (`:366-400`), marked inferred. The wave page names both and says they sit beside `story:verified-caller-token`'s hunk and `story:listener-tls`'s constructor.
3. Fixed. `story:several-writers-one-postgres` now lists the `status.rs:533-538` `planned` row, `status.json` and `status.md`. The wave page lists it under U6.
4. Fixed. `story:listener-tls` now lists `status.rs`, `status.json` and `status.md`.
5. Fixed. `story:spec-change-over-stored-rows` now lists the same three files.
6. Fixed. `story:event-egress-webhook` now lists the same three files.
7. Fixed. `story:dead-letters-kept-and-replayed` now lists the same three files.

The wave page's Surfaces section now names `status.rs`, `status.json` and `status.md` as shared by U3, U4, U5, U7, U8 and U6. It says the coordinator merges them by hand and regenerates them after each merge.

**New findings:** none.
- `story:verified-caller-token` now depends on `story:listener-tls` (D3 B). The edge is present, and its acceptance line `a_token_run_off_loopback_exits_2` reads `--tls-cert` and `--tls-key`. The shared `main.rs:647-657`, `Serving`, `cli.rs` and `listener.rs` hunks are named, and the merge order puts U3 before U4.
- The U7 before U4 order rests on the shared `calls.rs` helpers. It has no `depends_on` edge, but the wave page gives the file as its reason, which is allowed.
- The wave page now lists wave 30's other overlaps as line moves only. Wave 30 does not list `status.rs`, so nothing collides there.
- Every `postgres::` acceptance line in U5, U6, U7 and U8 reads the lane's 60 s kill that U1 creates. All four have a `depends_on` edge to U1.
- The story bodies contain no person, organisation, host or home path.

**What I read:** all 8 stories and the wave-31 page, plus `review-result:wave31-par-r1` and the full 278f3126 diff. I used `aep plan artifact show`, `aep plan artifact graph`, `aep plan artifact waves`, `git show 278f3126 --stat` and `git show 278f3126 -- .engineering/planning/story/`. I also read the wave-30 page and grepped the bodies for `status.rs` and `AGENTS.md`.

Surfaces: 8 items placed from cited surfaces, 0 resting on inferred surfaces alone, 0 unplaceable. I traced 8 acceptance reads to a producer in the set (U6 to U5, U7 and U8; U4 to U3; U5, U6, U7 and U8 to U1), and a `depends_on` edge records all 8.

**What I could not establish:**
- Which `AGENTS.md` sections U3 to U8 would touch. Only U2 lists the file. This is the same gap as in round 1, and I do not count it as a finding.
- Whether 120 s is enough once U6's SIGSTOP and failover tests sit in the PostgreSQL lane. That belongs to the test-budget critics, outside my lane.

```findings
[]
```

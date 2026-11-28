---
format: aep.planning-md/3
id: review-result:wave32-des-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on wave 32
relations:
- reviews: story:postgres-lane-runs-locally
- reviews: story:conform-awaits-system-steps
- reviews: story:leader-exits-promptly-on-sigterm
- reviews: story:seed-under-a-lease
revision: 1
---
needs-revision

- story:leader-exits-promptly-on-sigterm — its acceptance asks for 20 separate runs of a PostgreSQL-lane test as evidence, and the wave page says those runs "use U1's local lane" and orders U1 before U3; U1 alone creates that lane, and no `depends_on` edge runs from U3 to U1. Close it by adding `depends_on: story:postgres-lane-runs-locally` to U3. The alternative is to state in U3's acceptance that the runs use any cluster through `SPINDLE_TEST_DATABASE_URL`, and to cut the sentence at wave-32.md:73-74. The edge makes U1 → U3 → U4 a chain with U2 free. Each link has a reason beside it, so I do not read that as a serialising defect. Citations: .engineering/planning/story/leader-exits-promptly-on-sigterm.md:69, .engineering/waves/wave-32.md:73-74, .engineering/planning/story/postgres-lane-runs-locally.md:26.

**What I read.** I read all 4 stories and the wave page whole, plus `postgres-lane-margin`, `serve-loop-signal-exit-answers` and `seed-once-on-durable-state` as neighbours. I ran `aep plan artifact relations`, `graph` (filtered to the set and its neighbours) and `waves`, and `aep plan artifact validate`, which printed `valid`. Validate carries only unrelated reminders about old review results. I walked the one declared edge inside the set (U4 `depends_on` U3, plus the `serves` edges to `vision:spindle`) and found no cycle. I did not read the graph outside the set beyond the neighbours above. I traced 3 acceptance reads to a producer in the set: U4's lane tests to U3's exit path, which an edge records; U3's 20 evidence runs to U1's lane, which no edge records (the finding above); and U4's `task check:postgres` runs, which work on an externally supplied database, so they need no edge.

**Checked and not findings**
- U3 and U4 both touch `Leases::renewing`, `Kept`, `Lease` in `postgres.rs`, `serve.rs`, `cli.rs`, the generated `cli.md` and `postgres/replicas.rs`. U4 carries `depends_on` U3 and the wave page gives the file reason (wave-32.md:75-83), so this is the order-versus-split trade-off.
  - Order is kept: it costs U4 waiting on U3, and U4 is built on U3's exit path.
  - Splitting the lease thread's surface would also work, but the two items change different parts of it (U3 the release timeout, U4 the record channel), so I do not ask for it.
- U2 is a vertical slice that touches nothing in U1, U3 or U4 except shared prose rows (parallel-safety, not mine).
- There is no horizontal slice and no split abstraction.

**Out of my lane, for the other critics**
- (acceptance/scope) wave-32.md:68-72 tells U1 to raise `max_connections` if the measured peak comes within 50 of it, but U1's acceptance check at postgres-lane-runs-locally.md:53-55 fails unless the script holds the literal `max_connections=300`. A raised value would fail that check, and the wave page's duty is not in the story.
- (parallel-safety) U4's D3 cites `statement_timeout` at `postgres.rs:472`, which U3's D3 may change at `:463-485`. The edge covers the order, but U4's line numbers will be stale after U3 lands.

```findings
- file: .engineering/planning/story/leader-exits-promptly-on-sigterm.md
  line: 69
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its acceptance asks for 20 separate lane runs as evidence and the wave page says they use the local lane that story:postgres-lane-runs-locally creates, but no depends_on edge runs from this story to it; add depends_on story:postgres-lane-runs-locally, or state that the runs use any cluster through SPINDLE_TEST_DATABASE_URL (cite wave-32.md:73-74, postgres-lane-runs-locally.md:26)"
```

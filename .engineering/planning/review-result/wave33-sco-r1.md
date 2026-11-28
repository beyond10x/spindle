---
format: aep.planning-md/3
id: review-result:wave33-sco-r1
kind: review-result
status: active
title: Plan critic (scope), round 1, on wave 33
relations:
- reviews: story:listener-limits
- reviews: story:new-leader-write-while-old-paused
- reviews: story:seed-record-after-a-component-is-removed
- reviews: story:entity-invariant-over-stored-rows
revision: 1
---
approve

**What I read:** 6 artifacts. The parent `vision:spindle` first, then U1's second parent `epic:production-service`, then the 4 stories (`story:listener-limits`, `story:new-leader-write-while-old-paused`, `story:seed-record-after-a-component-is-removed`, `story:entity-invariant-over-stored-rows`). Each was read with `aep plan artifact show`, in the worktree `<local path>` at fc19c7d9.
- To check for overlapping claims I read the neighbours `story:spec-change-over-stored-rows`, `story:seed-under-a-lease` and `story:several-writers-one-postgres`, and `<local path>`. I also ran `aep plan artifact graph`, `aep plan artifact relations` and `aep plan artifact validate` (result: valid, nothing against the 4 stories), and grepped `.engineering/planning/story`.
- **Promises:** 5 in the parent (Outcome, O1 to O4) and 2 exclusions. 2 trace to items in this set:
  - The Outcome ("executes the model's commands") traces to U1 and U2.
  - O1 traces to U4 (a spec change judged against stored rows) and U3 (a later release that removes a component).
  - O2, O3 and O4 are not claimed here. That is expected: this is a wave picked from the vision, not a breakdown of it, and the graph has other items for those objectives.
- **Exclusions:** no story touches either one (`service-sdk`, capability lists written by hand).
- **The epic:** its promise "bounded connections and request time" traces to U1.
- **Overlaps:** no story claims the same outcome as another in the set or in the graph. U4 adds entity invariants, which `story:spec-change-over-stored-rows` never listed. U3 adds the case of a removed component, which `story:seed-under-a-lease` names through links in the docs. U2 adds the 503 for a store deadline, which `story:several-writers-one-postgres` does not claim.
- **Narrowing:** each narrowing is stated in the story: U2 `## Not covered` (reads stay 501), U2's D2 A (the window stays open), U3's D3 A (one schema per leased deployment). U1's control-token compare was taken by D5 A, so it is a recorded choice, not reach beyond the parent.
- **Other brief checks** (private names and paths, network, Python, lane naming): clean. Every test that needs PostgreSQL names `task check:postgres`, every test is loopback-only, nothing in the stories adds Python, and the 4 story bodies name no person, organisation, host or home path.

**What I could not establish:**
- **PostgreSQL lane load, out of my lane:** no single test over about 10 s goes unstated. U2's `a_write_behind_the_publication_lock_answers_503` states "under 15 s", and the rewritten `a_write_behind_a_foreign_table_lock_fails_in_time_without_losing_the_lease` keeps its existing 11 s bound and 12 s lock. Taken together, the declared bounds add up to about 85 s of test time (U2 20 s, U3 6 × 10 s, U4 5 s). That lands on a lane that needs about 100 CPU-seconds of a 4-CPU runner (`story:postgres-lane-margin`, Why). U3's times are "expected, not timed", so only the before-and-after timing each unit must report shows whether the 100 s parallel bound holds.
- **Out of my lane (design):** the Scope "Tests" bullet of `story:listener-limits` still names `connections_over_the_cap_are_refused` and leaves out the tests added later. Its safety fact names `max_buf_size`, while Acceptance uses `max_header_size`.
- **Out of my lane (acceptance):** no acceptance test in U3 covers the Outcome's "Several records … differing they exit 1 naming the tables" for orphan records. U4 has no test of the stdin `run` path that `story:spec-change-over-stored-rows` covered.

```findings
[]
```

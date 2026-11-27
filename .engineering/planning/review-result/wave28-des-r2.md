---
format: aep.planning-md/3
id: review-result:wave28-des-r2
kind: review-result
status: active
title: Plan critic (design), round 2, on wave 28
relations:
- reviews: story:console-routes-off
- reviews: story:structured-json-logs
- reviews: story:metrics-series-bounded
- reviews: story:seed-readiness
- reviews: story:health-and-graceful-drain
- reviews: story:entity-event-history
- reviews: story:events-record-their-caller
- reviews: story:environment-binds-external-decisions
- reviews: story:replay-with-decisions
- reviews: story:one-time-response-admission
revision: 1
---
approve

Round 1's one finding is resolved. It was on `story:entity-event-history` and its `/api/history` route, which `story:console-routes-off` did not cover.

- **How it was closed:** I asked for one of two fixes and the revision took the second. `story:console-routes-off` now states "every route of `Console::ALL`" in its Outcome (`.engineering/planning/story/console-routes-off.md`, Outcome). Its acceptance `console_routes_off_answer_404_with_the_secret` iterates `Console::ALL` and names `GET /api/history` as a later route it covers.
- **No edge needed:** the acceptance reads the list, not another story's output, so `story:console-routes-off` needs no `depends_on` on `story:entity-event-history`. The wave page puts U5 before U1 (merge order U7, U6, U5, U3, U1, U4, U2), so the iterated list holds `/api/history` when U1 merges.
- **Re-review:** I found no new design defects.
  - **Cycles:** none. The two needs-first edges in the set, `story:health-and-graceful-drain` to `story:seed-readiness` and `story:entity-event-history` to `story:events-record-their-caller`, are both recorded.
  - **Chains:** only U4 and U5 chain, and each has its edge. U1, U2, U3, U6 and U7 are not chained to anything.
  - **Split abstractions:** none. The two stories in U7 each stand alone: one is the prefix admission fix, the other lifts the `--scenario` and `--decisions` conflict.
  - **Slices:** none. Every story is end to end.

**What I read:**
- All 10 stories in full with `aep plan artifact show`, plus the wave page and my round-1 result.
- I ran `aep plan artifact graph` and `validate` (valid). I walked the 7 `depends_on` and `informed_by` edges that touch the set and followed them outside it. I traced 4 acceptance reads of sibling-produced state. Three have an edge or are covered by the revised acceptance: `/__spindle/ready` from `story:seed-readiness`, the caller from `story:events-record-their-caller`, and `Console::ALL` iteration. One is unrecorded; the stderr lines of other units are the fourth.
- No story body holds a person, organisation, host or home path.

**What I could not establish:**
- Out of my lane (parallel safety): the `Replays::run` overlap between the two U7 stories carries no edge. Their order is on the wave page only.
- Out of my lane (parallel safety): `story:structured-json-logs`'s acceptance `every_stderr_line_is_one_json_object` covers stderr lines that other units add. Only the wave page's merge order (U2 last) records that.
- Out of my lane (acceptance): the times of the new spawned-binary tests are unmeasured.

```findings
[]
```

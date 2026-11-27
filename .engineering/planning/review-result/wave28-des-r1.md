---
format: aep.planning-md/3
id: review-result:wave28-des-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on wave 28
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
needs-revision

story:entity-event-history — its D1 says `/api/history` joins `Console::ALL` "so `story:console-routes-off`'s switch covers it", but no `depends_on` or other edge records that reliance. Neither story has an acceptance line that checks `/api/history` answers 404 with `--console off`. `story:console-routes-off` names and tests only the four routes of today's `Console::ALL`, and the merge order lands U5 before U1. Close it in one of two ways. Either add a `depends_on` from `story:entity-event-history` to `story:console-routes-off`, with the order U1 before U5, and an acceptance line `the_history_route_is_off_with_the_console_off`. Or have `story:console-routes-off` state "every route of `Console::ALL`" and test it by iterating `Console::ALL`, so U5's route is covered when it lands. — .engineering/planning/story/entity-event-history.md:126-127, .engineering/planning/story/console-routes-off.md:56, crates/spindle-ingress/src/console.rs:30

**What I read:** 10 stories in full with `aep plan artifact show`, plus `story:serve-loop-signal-exit-answers` (implemented, outside the set) and the wave page. I ran `aep plan artifact relations`, `graph`, `waves` and `validate` (valid). The graph is clean: no cycle in the 24 edges that touch the set, and I also followed edges out to `epic:*`, `vision:spindle` and three stories outside the set. The only chains are U4 and U5, and each has a recorded `depends_on`. I traced 3 acceptance reads of state another story in the set produces. Two have an edge: `story:health-and-graceful-drain` to `story:seed-readiness`'s `/__spindle/ready`, and `story:entity-event-history` to `story:events-record-their-caller`'s caller. The third is the `/api/history` read above, which has none. No story body holds a person, organisation, host or home path.

**What I could not establish:**
- Out of my lane (parallel safety): `story:one-time-response-admission` and `story:replay-with-decisions` both rewrite `Replays::run` and carry no edge. The wave page orders them inside U7 (one-time first), but the order is not recorded in the graph.
- Out of my lane (parallel safety): `story:structured-json-logs` merges last so that its implementor can route the stderr lines other units add, and the wave page records that order. No other story body names a stderr line it adds, so I could not tell whether that coupling is real. Its acceptance `every_stderr_line_is_one_json_object` holds only after the merge.
- Out of my lane (acceptance): the test times of the new spawned-binary tests are unmeasured, as the scopes themselves say.

```findings
- file: .engineering/planning/story/entity-event-history.md
  line: 126
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its D1 relies on story:console-routes-off's switch to cover the new /api/history console route, but no edge records that and neither story has an acceptance line that checks /api/history answers 404 with --console off; add a depends_on to story:console-routes-off with an acceptance line, or make console-routes-off state and test every route of Console::ALL"
```

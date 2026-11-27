---
format: aep.planning-md/3
id: review-result:wave28-sco-r3
kind: review-result
status: active
title: Plan critic (scope), round 3, on wave 28
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

I read all ten stories and the wave page in full and found no findings in my lane.

Round-2 finding, `story:health-and-graceful-drain`: resolved. The Scope `**Tests:**` line now lists `the_liveness_route_needs_no_observe` with "(`/__spindle/live` only; the ready route is `story:seed-readiness`'s)". The Acceptance already said the same. The `/__spindle/ready` no-observe and guard outcome is claimed once, by `story:seed-readiness`.

Round-1 findings, all four resolved:
- **Selection and `story:one-time-response-admission`:** the Selection now claims it as a consumer-reported replay fault that shares `Replays::run`.
- **Held-out stories:** the wave page lists the seven held-out story ids.
- **`story:console-routes-off`:** Outcome and Acceptance now speak over `Console::ALL`, iterated in the test, and name `/api/history` as an example.
- **Health versus seed-readiness:** see the round-2 finding above.

Fresh pass over the set:
- I traced each of the ten stories to the operator's order "resolve all missing spindle features". Each serves `vision:spindle`.
- Seven stories are held out by name, and `story:sensitive-fields-redacted` is named as waiting on an ESS release.
- No two stories claim one outcome. The pairs I checked:
  - seed-readiness and health-and-graceful-drain;
  - events-record-their-caller and entity-event-history, whose D4 splits `GET /api/events` from the per-instance read;
  - one-time-response-admission and replay-with-decisions, one a bisection fix and the other a flag combination.
- The replacement of wave 27's D1 for `story:serve-loop-signal-exit-answers` is recorded in D4 of `story:health-and-graceful-drain`.
- A grep of all ten bodies and the wave page found no person, organisation, host, URL or home path.

**What you read:** 11 artifacts, namely the ten stories and the wave page, by `aep plan artifact show <id>` and `cat`. I also read `review-result:wave28-sco-r1` and `-r2`, the scope-critic procedure and the rubric.

**What I could not establish:**
- No triage record is in the tree, so I could not check the 98-story count or the seven held-out stories as a number.
- Out of my lane (acceptance): in `story:health-and-graceful-drain`, Scope names `readiness_reads_503_once_draining` where Acceptance names `readiness_reads_503_once_sigterm_arrives`.
- Out of my lane (acceptance): in `story:metrics-series-bounded`, Scope names `an_aggregated_scrape_of_300_commands_writes_at_most_n_series` where Acceptance names `an_aggregated_scrape_does_not_grow_with_the_specification`.
- Out of my lane (design): `story:health-and-graceful-drain` Outcome says readiness is `200` "once its stores are open", and no acceptance checks stores being open.

```findings
[]
```

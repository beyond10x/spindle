---
format: aep.planning-md/3
id: review-result:wave28-sco-r2
kind: review-result
status: active
title: Plan critic (scope), round 2, on wave 28
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

- `story:health-and-graceful-drain` — the Scope `Tests` line still lists `the_health_routes_need_no_observe` over both routes, but the Acceptance now has `the_liveness_route_needs_no_observe` for `/__spindle/live` only. The `/__spindle/ready` no-observe and guard outcome belongs to `story:seed-readiness`, so an implementor reading Scope would write a second claim on it. Rename the Scope test to `the_liveness_route_needs_no_observe` and limit it to `/__spindle/live`. Round-1 finding 4 is only partly fixed: the Acceptance changed, the Scope did not. — `aep plan artifact show story:health-and-graceful-drain` § Scope, the `**Tests:**` line for `crates/spindle-ingress/tests/spindle-ingress/health.rs`; the Acceptance line is in `<local path>`

Round-1 findings in my lane:
- **Wave page, Selection omits `story:one-time-response-admission`:** resolved. `wave-28.md:15-16` now claims it as a consumer-reported replay fault that shares `Replays::run`.
- **Wave page, seven held-out stories unnamed:** resolved. `wave-28.md:19-23` lists the seven ids, and all seven exist as stories. It also names `story:sensitive-fields-redacted` as waiting on ESS.
- **`story:console-routes-off`, routes fixed at four:** resolved. Outcome and Acceptance now speak over `Console::ALL`, iterated in the test, and name `/api/history` as an example of a later route.
- **`story:health-and-graceful-drain`, the same outcome as `story:seed-readiness`:** partly resolved. See the finding above.

I read no person, organisation, host or home path in the ten story bodies or the wave page. A grep for `/home/` and `spindle-plan` found nothing.

**What you read:**
- 21 artifacts: the ten stories, `epic:production-service`, `review-result:wave28-sco-r1`, and the seven held-out stories plus `story:listener-limits` and `story:release-artifacts-signed`. I read those nine by header only.
- `.engineering/waves/wave-28.md`.
- The `a6778760..370e9da6` diff for two stories.
- Commands: `aep plan artifact show` on each, and `git diff` / `git log` in the worktree.

I extracted 11 promises: the 10 selected stories in the wave page, and the seven held-out ones read as one recorded decision. I traced 11 to a claim or a named omission.

**What I could not establish:**
- No triage record exists in the tree, so the "98 open stories" count and the seven held-out count stay unchecked.
- `story:listener-limits` and `story:release-artifacts-signed` decompose `epic:production-service` and appear in neither the wave nor the held-out list. The order covers all 98 open stories, so I did not count this as a gap.
- `story:entity-event-history` decomposes `epic:split-run`, but that epic promises no per-instance history. This is a pre-existing edge and did not set my verdict.
- Out of my lane, parallel safety: the round-1 note about the `answer.rs` surface is now in the Surfaces section, so it is resolved.

```findings
- file: aep plan artifact show story:health-and-graceful-drain
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Scope Tests line still lists the_health_routes_need_no_observe over both health routes while the Acceptance now limits the no-observe test to /__spindle/live (the_liveness_route_needs_no_observe), so the ready-route outcome that story:seed-readiness claims is claimed twice; rename and limit the Scope test to /__spindle/live"
```

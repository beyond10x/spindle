---
format: aep.planning-md/3
id: review-result:wave28-sco-r1
kind: review-result
status: active
title: Plan critic (scope), round 1, on wave 28
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

- `.engineering/waves/wave-28.md` — the Selection names nine of the ten stories; `story:one-time-response-admission` is in U7 but no sentence of Selection claims it ("decisions answered from the environment and in replay" covers `replay-with-decisions` only). Add it as a consumer-reported replay fault that shares `Replays::run` — `.engineering/waves/wave-28.md:13-19`
- `.engineering/waves/wave-28.md` — the order "resolve all missing spindle features" is narrowed to ten stories, and "Seven stories that change the architecture wait for the operator's picks" names none of the seven, so the omission is not recorded as a decision. List the seven story ids — `.engineering/waves/wave-28.md:19-20`
- `story:console-routes-off` — Outcome and Acceptance name four routes (`/api/reset`, `/api/events`, `/api/model`, `/api/served`). `story:entity-event-history` D1 adds `/api/history` to `Console::ALL` "so `story:console-routes-off`'s switch covers it", and U5 merges before U1. No acceptance in either story makes `/api/history` answer 404 with `--console off`. State the promise over `Console::ALL`, not a fixed list of four routes — `aep plan artifact show story:console-routes-off` § Acceptance; `aep plan artifact show story:entity-event-history` § Decided when scoped D1
- `story:health-and-graceful-drain` — `the_health_routes_need_no_observe` asserts the same outcome for `/__spindle/ready` (answers without `--observe`, 403 without the secret) that `story:seed-readiness` already claims in `the_seed_state_needs_no_observe` and `a_trusted_caller_run_guards_the_readiness_route_as_its_console`. Two stories claim one outcome and both will be marked done. Limit the health test to `/__spindle/live` — `aep plan artifact show story:health-and-graceful-drain` § Acceptance

**What you read:** 11 artifacts.
- Ten stories, each by `aep plan artifact show <id>`.
- `epic:production-service`, `epic:runtime-observability-depth` and `epic:split-run`, in full.
- `.engineering/waves/wave-28.md`.
- `aep plan artifact graph`, `relations`, `list --kind story`.

I extracted 10 promises from the wave page and the operator's order: the 9 listed in Selection plus the 7 held-out stories. I traced 9 of the 10; the one-time response story is the one I could not trace. I also grepped the ten story bodies and the wave page for persons, hosts and home paths and found none.

**What I could not establish:**
- No triage record exists in the tree. I could not check the claim of 98 open stories or the count of seven held-out ones.
- `story:entity-event-history` decomposes `epic:split-run`, but that epic's table and outcomes promise no per-instance history. The edge is pre-existing. I did not set the verdict on it.
- `story:entity-event-history` Outcome names "component, entity, identity", while its D1 route takes only `entity=&identity=`. This is the story's Outcome against its own decision, so I left it for the design and acceptance critics.
- `story:entity-event-history` Why says "in its words above" with no quotation above it. Left to the acceptance critic.
- Out of my lane, parallel safety: `story:events-record-their-caller` scope puts `crates/spindle-ingress/src/answer.rs` (the request id) in its surface, but the wave page's Surfaces section omits that file.

```findings
- file: .engineering/waves/wave-28.md
  line: 13
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Selection names nine of the ten stories; story:one-time-response-admission (U7) is claimed by no sentence of it, so add it as a consumer-reported replay fault sharing Replays::run"
- file: .engineering/waves/wave-28.md
  line: 19
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the order 'resolve all missing spindle features' is narrowed and 'Seven stories that change the architecture wait' names none of the seven, so the held-out stories are not recorded as a decision; list their ids"
- file: aep plan artifact show story:console-routes-off
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "Outcome and Acceptance name four console routes, while story:entity-event-history D1 adds /api/history to Console::ALL under this story's switch and merges first; no acceptance claims that route answers 404 with the console off, so state the promise over Console::ALL"
- file: aep plan artifact show story:health-and-graceful-drain
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the_health_routes_need_no_observe asserts for /__spindle/ready the same no-observe and trusted-caller-guard outcome that story:seed-readiness already claims; limit this test to /__spindle/live so each outcome is claimed once"
```

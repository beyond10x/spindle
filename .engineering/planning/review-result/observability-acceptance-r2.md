---
format: aep.planning-md/3
id: review-result:observability-acceptance-r2
kind: review-result
status: active
title: Acceptance critic, round 2, runtime observability
relations:
- reviews: epic:runtime-observability
- reviews: story:observe-ingress-model
- reviews: story:ingress-request-log
- reviews: story:ingress-prometheus-metrics
- reviews: story:ingress-observe-panel
- reviews: story:external-answers-observed
- reviews: story:live-topology
revision: 1
---
approve

**What I read:** 7 of 7 ids (the epic and the 6 stories). I ran `aep plan artifact show` on each and on `review-result:observability-acceptance-r1`. I checked the Taskfile `check:*` names, the telemetry `carried` keys, `examples/rally/ess`, and `recorded`, `failed`, `externals` and `latest` in `docs/design/proposals/observe-ingress.yaml`.

Round 1's one finding, on `story:live-topology`, is fixed. The new check is `invokes_edges_follow_the_counters`. On the rally example, each event binding must have one `invokes` edge, and its count must equal that binding's `spindle_bindings_total{carried="delivered"}`. After the match is opened and ticked it must match, and it must grow on the next tick. That check is at `.engineering/planning/story/live-topology.md`, in the Acceptance section. Before the work the edge does not exist, and after it the counts match, so the transition is stated. The `carried` key `delivered` exists in `crates/spindle-telemetry/src/port.rs:166`. The ignored multi-specification test is now an extra check and no longer the only `invokes` check.

I found no new acceptance defects. The scripted-sequence numbers in `story:ingress-prometheus-metrics` agree with the sequence in `story:ingress-request-log`. That sequence is 7 requests, 2 of them unrouted (`GET /nowhere` and `DELETE /tasks`), and the declared 410 is counted as `4xx`.

**What I could not establish:**
- `ess/fixtures/observe/requests`, `metrics` and `topology` do not exist yet. Their content (`scripted.json`, `families.json`, `tasks.json`) is a check I cannot run until `story:observe-ingress-model` builds them.
- I did not run `ess specify validate` on `observe-ingress.yaml`.
- Out of my lane, and they did not set the verdict. The panel banner shows five counts, while `RequestClass` has six variants including `failed`, so a `failed` request has no banner count (scope or design). The `invokes` acceptance assumes the rally example's event bindings invoke commands in process, which belongs to scope or design.

```findings
[]
```

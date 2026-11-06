---
format: aep.planning-md/3
id: review-result:observability-scope-r2
kind: review-result
status: active
title: Scope critic, round 2, runtime observability
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

**What you read:** 7 artifacts, whole bodies, with `aep plan artifact show`: the epic first, then the six stories. I also ran `aep plan artifact graph` and read the four round 1 results, `review-result:observability-{acceptance,design,parallel,scope}-r1`.
- I wrote the epic's promise list before reading any story. It has 22 promises: the flag and its `--observe-requests` capacity, the four routes and their contents, the `404` off state, the start-time refusal, and the 11 decisions in its "Decisions taken" section. I traced all 22 to a story.
- Each round 1 scope finding is closed in the revised text:
  - Resets: `ingress-prometheus-metrics` now states that a reset leaves counters and the log intact, and its acceptance `a_reset_leaves_counters_and_the_log_intact` checks it.
  - Durations: `ingress-request-log` now states the measurement point, and `a_request_held_on_the_host_thread_is_timed_with_its_wait` checks it.
  - The `observe` key on the stdout listening line is gone. The story now says the line is unchanged, and `the_listening_line_is_unchanged_by_observe` checks it.
- "View reads" is claimed through `kind="view"` on `spindle_http_requests_total`, and the metrics outcome says so ("there is no separate view family"). The narrowing is recorded.
- The multi-specification topology promise is carried by `live-topology` as an `#[ignore]` test tied to `story:one-ingress-several-specifications`, and the story says so. The epic says the same, so nothing is narrowed silently.
- No reach beyond the parent. Each story's outcome traces to an epic sentence, and nothing lands in the epic's "Out" list. The extra `depends_on` edges on `ingress-request-log` and `external-answers-observed` order them against other active stories and add no outcome.
- No two stories claim one outcome. `ingress-prometheus-metrics` declares `spindle_external_causes_total` empty, and `external-answers-observed` fills it. `live-topology` reads those numbers and does not restate them.
- The graph shows nothing outside the set claiming part of the epic.

**What you could not establish:**
- Whether the epic's Origin phrase "the emitted events and their channels" asks for more than the `carries` edges and binding nodes in `live-topology`. The epic's Outcome lists no channel node, so I counted it covered.
- Out of lane, acceptance: `ingress-observe-panel` checks that every request the panel makes is `./requests` or `./metrics`. `live-topology` adds a screen that polls `./topology`, which would break that check.
- Out of lane, acceptance: the decision "no `Access-Control-Allow-Origin` header" has no check in any story.
- Out of lane, acceptance: only a binding route, not an ESS-derived route, is tested under the reserved prefix.
- Out of lane, design: the epic's story table omits the `external-answers-observed` to `ingress-observe-panel` edge that the story body records.
- I did not run `ess specify validate` on the proposal file.

```findings
[]
```

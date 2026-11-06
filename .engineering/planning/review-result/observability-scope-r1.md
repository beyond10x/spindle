---
format: aep.planning-md/3
id: review-result:observability-scope-r1
kind: review-result
status: active
title: Scope critic, round 1, runtime observability
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
needs-revision

- story:ingress-prometheus-metrics — the epic decides "**Resets do not clear** the log or any counter; Prometheus counters stay monotonic", and no item claims it; the metrics story only counts resets and drops `spindle_entity_rows`, so add a clause to its outcome and a check that a reset leaves counters and the log intact — .engineering/planning/epic/runtime-observability.md:112
- story:ingress-request-log — the epic decides "**Durations** are measured on the listener thread from the request's head read to the answer handed back, so queueing behind a running command is included", and the story's outcome, scope and acceptance never say where the clock starts or stops, only that it is read when a recorder exists. State the measurement point in the outcome (the model yaml says it at observe-ingress.yaml:335, but no behaviour claims it) — .engineering/planning/epic/runtime-observability.md:114
- story:ingress-request-log — the outcome adds `"observe": "http://127.0.0.1:<port>/__spindle/"` to the run's stdout listening line, and no sentence of the epic's Outcome, Scope, Origin or consumer asks mentions a change to that line. Either drop it or add it to the epic's Scope (the line it extends is printed at `crates/spindle-cli/src/serve.rs:253`) — .engineering/planning/story/ingress-request-log.md:43

Out of lane, so it does not set the verdict:
- Acceptance: the epic's "counters stay monotonic" and the duration decision have no checkable assertion yet.
- Design: the metrics and panel stories both read `./metrics`, and it is not clear why the panel needs it.

Unease, not a finding: the epic's "view reads" metric has no family of its own. The only trace is `kind="view"` on `spindle_http_requests_total`, and the telemetry crate has no view counter. I counted it covered through that label. The epic or the metrics story could say so in one line.

**What you read.**
- 7 artifacts: the epic and the six stories, each with `aep plan artifact show`. I also ran `aep plan artifact graph`.
- `docs/design/proposals/observe-ingress.yaml`, partly: the types for requests, classes, answer sources and metric families.
- The existing `ess/fixtures/observe` tree, and `serve.rs` and the telemetry source by grep.
- I extracted 20 promises from the epic and traced 18 to an item. The 2 untraced are the resets decision and the durations decision.
- Nothing else in the graph claims any part of the epic. The only related artifacts are `epic:observe` and the implemented `observe-*` stories, none of which touch the reserved routes.
- No two items claim one outcome. The `asks` edges and the `externals` field consume what `story:external-answers-observed` records, and do not claim it again.

**What I could not establish.**
- Whether `spindle_http_requests_total` with `kind="view"` is what the epic means by "view reads". The epic does not define the term.
- `ess/fixtures/observe/requests`, `metrics` and `topology` do not exist yet. I judged the stories by their text only.

```findings
- file: .engineering/planning/epic/runtime-observability.md
  line: 112
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "story:ingress-prometheus-metrics: the epic's decision \"Resets do not clear the log or any counter; Prometheus counters stay monotonic\" is claimed by no item; the metrics story only counts resets and drops entity rows, so its outcome should state that a reset leaves counters and the log intact"
- file: .engineering/planning/epic/runtime-observability.md
  line: 114
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "story:ingress-request-log: the epic's decision that durations run from the request's head read to the answer handed back, queueing included, is claimed by no item; the story's outcome never states where the clock starts and stops"
- file: .engineering/planning/story/ingress-request-log.md
  line: 43
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "story:ingress-request-log: the outcome adds an `observe` key to the stdout listening line, which no sentence of the epic's Outcome, Scope, Origin or consumer asks mentions; drop it or add it to the epic's Scope"
```

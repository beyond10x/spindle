---
format: aep.planning-md/3
id: review-result:observability-design-r1
kind: review-result
status: active
title: Design critic, round 1, runtime observability
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

I read the set whole and found nothing that would change what was drafted. The shape holds on all five defects.

- **Cycle:** none. The `depends_on` edges run model → request-log → {metrics, panel} → external-answers (needs metrics) → live-topology. The other `depends_on` edges are request-log → `story:http-router-is-transport-free`, and that story's own edges to `story:run-over-http-ingress` and `story:wasm-host-builds`. Everything else is `serves` or `decomposes`, which mean shaped-by and part-of, not needs-first.
- **Chain:** not a queue. Metrics and panel run in parallel at order 3, and the epic records why each edge exists.
- **Split abstraction:** none.
  - The model story is the noun contract, demonstrable on its own by validation, drift check and fixtures. Request-log, metrics and panel each serve something end to end.
  - The `externals` field and the `spindle_external_causes_total` family are declared early. Each is stated as empty until `story:external-answers-observed` fills it, so the seam is explicit and each earlier story still stands alone.
- **Hidden dependency:** every acceptance read I traced to a producer in the set has a `depends_on` edge, directly or through an edge already present.

  | Reader | Reads | Producer |
  |---|---|---|
  | request-log | `ess/fixtures/observe/requests/scripted.json`, `spindle_observe_api::RequestLog` | model |
  | metrics | `families.json`, the scripted request sequence | model, request-log |
  | panel | `RequestLog` fixture | model |
  | external-answers | `spindle_external_causes_total`, the record's `externals` | metrics, request-log |
  | live-topology | `topology/tasks.json`, `spindle_http_requests_total`, `spindle_external_causes_total`, `spindle_events_appended_total`, the ring | model, metrics, request-log, external-answers |

  The ingress-request-log → `story:http-router-is-transport-free` edge is present, and that story is active.
- **Horizontal slice:** none beyond the model story, which the workspace rule requires first and which passes on its own.

The proposed domain `docs/design/proposals/observe-ingress.yaml` matches the stories' vocabulary. It has six request classes, six edge kinds, `AnswerSource` as world, file, default or unanswered, and `OperationKind` as command, view or console.

**What you read:** 7 artifacts, whole bodies, with `aep plan artifact show`, `relations`, `graph` and `validate`. `validate` reported valid across all 136 artifacts. I walked 32 edges touching the set, including edges out of the set to `vision:spindle` and `story:http-router-is-transport-free`, and traced 12 acceptance reads to producers in the set, all of them recorded by an edge. I skimmed the proposal's kinds and variants only, not its full text.

**What you could not establish:**
- **Out of lane, parallel safety:** `ess/fixtures/observe/topology` appears in both the model story's scope and the live-topology scope. `crates/spindle-ingress/src/observe.rs` and `crates/spindle-cli/tests/observe.rs` are shared by several stories, and the epic documents the one-line-arm overlap.
- **Out of lane, acceptance:**
  - Fixtures drafted before the behaviour exists (`scripted.json`, `tasks.json`) become exact-match expectations later.
  - The panel acceptance says it reads `./metrics`, but no panel acceptance line reads the metrics output.

```findings
[]
```

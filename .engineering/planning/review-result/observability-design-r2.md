---
format: aep.planning-md/3
id: review-result:observability-design-r2
kind: review-result
status: active
title: Design critic, round 2, runtime observability
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

No finding would change what was drafted. The revised set holds on all five defects.

- **Cycle:** none. The `depends_on` edges run model → request-log → {metrics, panel} → external-answers → live-topology. request-log also needs three stories outside the set, `http-router-is-transport-free`, `outcome-scoped-refusal` and `fixed-caller-per-grant`, and none of them points back. `serves` and `decomposes` mean shaped-by and part-of, so they are not needs-first.
- **Chain:** the set is not a queue. Metrics and panel run in parallel. external-answers has a `depends_on` edge to the panel only because both edit `crates/spindle-ingress/src/observe.rs`, and the story's Order section says so. That is the file-sharing trade-off the rubric describes, and the reason is written beside the edge.
- **Split abstraction:** none. The model story is the noun contract and passes alone on validation, drift check, fixtures and vitest guards. The `externals` field and `spindle_external_causes_total` are declared early and stated as empty until external-answers fills them. Each earlier story therefore still stands alone.
- **Hidden dependency:** none. The round 1 gaps are closed.
  - metrics reads `families.json` from the model story, and the model edge is reachable through request-log.
  - topology reads `topology/tasks.json`, `spindle_http_requests_total`, `spindle_external_causes_total`, the events and bindings counters, and the ring. Edges run to the model, request-log, metrics and external-answers.
  - The `invokes` acceptance reads host counters that already exist. The Out-of-the-set story `one-ingress-several-specifications` is correctly an ignored test, not an edge.
- **Horizontal slice:** none. Request-log, metrics, panel, external-answers and topology each deliver an end-to-end behaviour. The model story is the contract-first step the workspace rule requires.

**What you read:** 7 artifacts (epic and six stories) in full, plus `review-result:observability-design-r1`, with `aep plan artifact show`, `relations`, `graph` and `validate`. `validate` reported valid across 140 artifacts. I walked about 36 `depends_on`, `serves` and `decomposes` edges, including those out of the set to `http-router-is-transport-free`, `outcome-scoped-refusal`, `fixed-caller-per-grant` and `vision:spindle`. I traced 13 acceptance reads to a producer in the set, and every one has an edge, directly or through request-log. I confirmed that `docs/design/proposals/observe-ingress.yaml` exists.

**What you could not establish:**
- The epic's Stories table lists external-answers as depending on "2, 3 (metrics)", but the story also has edges to the panel and to `fixed-caller-per-grant`. That is a body-consistency matter, not a shape defect, so I left it out of the findings.
- Out of my lane, parallel safety: `ess/fixtures/observe/topology` appears in both the model story and the topology story. `observe.rs` and `crates/spindle-cli/tests/observe.rs` are shared by several stories, and the epic and stories record this.
- Out of my lane, acceptance: the panel acceptance says it reads `./metrics`, but no acceptance line checks it. The fixtures `scripted.json` and `tasks.json` are drafted before the behaviour exists and become exact-match expectations later.

```findings
[]
```

---
format: aep.planning-md/3
id: review-result:observability-parallel-r1
kind: review-result
status: active
title: Parallel-safety critic, round 1, runtime observability
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

1. story:ingress-request-log — its scope lands on `crates/spindle-cli/src/cli.rs`, `crates/spindle-cli/src/serve.rs` and `crates/spindle-ingress/src/surface.rs` (cited, both bodies). The active `story:fixed-caller-per-grant` scopes all three too, and neither body mentions the other. Remedies: an ordering edge recording the shared files, or splitting the surface. — .engineering/planning/story/ingress-request-log.md:15,17,27 and .engineering/planning/story/fixed-caller-per-grant.md:14,18
2. story:ingress-request-log — it and the active `story:outcome-scoped-refusal` both land on `crates/spindle-ingress/src/surface.rs` (cited here, inferred there), and the body names neither it nor `crates/spindle-ingress/src/answer.rs`, whose answers it classifies. Remedies: an ordering edge, or splitting the surface. — .engineering/planning/story/ingress-request-log.md:27 and .engineering/planning/story/outcome-scoped-refusal.md:13
3. story:external-answers-observed — its `crates/spindle-cli/src/serve.rs` (cited) is also in the active `story:fixed-caller-per-grant`. Its only edges run to `ingress-request-log` and `ingress-prometheus-metrics`, so nothing orders it against that story even if finding 1 is fixed on the log story. Remedies: an ordering edge, or splitting the surface. — .engineering/planning/story/external-answers-observed.md:16 and .engineering/planning/story/fixed-caller-per-grant.md:14
4. story:external-answers-observed — it and `story:ingress-observe-panel` both edit `crates/spindle-ingress/src/observe.rs`. That file does not exist yet and is created by `ingress-request-log`. The panel adds the `ui` arm. This story attaches answers to the current record. No edge orders them: the panel depends on the model and the log, this story on the log and metrics. The epic names only the metrics, panel and topology overlap, and each body says "parallel with" only the pair it names. Remedies: an `external-answers-observed` → `ingress-observe-panel` edge giving the shared file as its reason, or splitting the recorder out of the dispatch file. — .engineering/planning/story/external-answers-observed.md:20,54,59 and .engineering/planning/story/ingress-observe-panel.md:17,70,75
5. story:ingress-observe-panel — the scope lists no file for its three Rust acceptance tests (`the_panel_is_one_self_contained_page`, `the_panel_may_be_framed`, `without_the_built_page_the_panel_answers_503`). The only candidate is `crates/spindle-cli/tests/observe.rs` (inferred), which `ingress-prometheus-metrics` claims and runs beside the panel. The collision is on a file neither story creates, and only the log story creates it. The body says it is parallel with metrics but does not name this file. Remedies: an ordering edge, or a separate test file for the panel. — .engineering/planning/story/ingress-observe-panel.md:51-55,75 and .engineering/planning/story/ingress-prometheus-metrics.md:15,76

**What you read:** 7 set artifacts, with 16 active stories and the drafts in `story:one-ingress-several-specifications`'s neighbourhood read for scope. The commands were `aep plan artifact list --status active`, `aep plan artifact list`, `aep plan artifact graph`, `aep plan artifact waves`, the files under `.engineering/planning/{epic,story}/`, and `ls`/`git ls-files` to confirm the cited paths exist.

**Placement:** 6 stories placed from cited surfaces, 0 inferred-only, 0 unplaced. The epic has no scope of its own. I traced 12 acceptance reads to a producer in the set, and `depends_on` records 11 directly. The twelfth is the metrics story's read of `families.json`, which the model story creates. It is ordered only transitively through `ingress-request-log`, which is enough for waves and not a finding.

**What I could not establish:**
- The panel's outcome says it reads `./metrics`, but its acceptance checks only the URL it requests, never what the route returns. I treated this as no reader beside its producer. Which story owns parsing that text is a question for the acceptance critic.
- `story:one-test-binary-per-crate` (active, `crates/spindle-cli/tests`) is named only in a conditional in `ingress-request-log`. The other four stories use `tests/observe.rs` but are ordered after the log, so I left it.
- Whether the model's shared files (`Taskfile.yml`, `ess/domains/observe.yaml`) collide with the non-active `story:observe-ui` is outside the set and not examined.
- Whether `ingress-request-log` and the other stories are well split is out of my lane.

```findings
- file: .engineering/planning/story/ingress-request-log.md
  line: 15
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "scope lands on cli.rs, serve.rs and surface.rs (cited, both bodies), the same three files as active story:fixed-caller-per-grant; neither body mentions the other and no edge orders them; add an ordering edge naming the shared files or split the surface"
- file: .engineering/planning/story/ingress-request-log.md
  line: 27
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "scope lands on crates/spindle-ingress/src/surface.rs, also scoped by active story:outcome-scoped-refusal (inferred there); no body or edge records it; add an ordering edge naming the file or split the surface"
- file: .engineering/planning/story/external-answers-observed.md
  line: 16
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "scope lands on crates/spindle-cli/src/serve.rs (cited), also scoped by active story:fixed-caller-per-grant; its edges reach only ingress-request-log and ingress-prometheus-metrics, so nothing orders it against that story; add an ordering edge naming the file or split the surface"
- file: .engineering/planning/story/external-answers-observed.md
  line: 20
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "both this story and story:ingress-observe-panel edit crates/spindle-ingress/src/observe.rs, a file not yet created (ingress-request-log creates it), with no depends_on edge between them and neither body naming the other; add an edge recording the shared file or split the recorder out of the dispatch file"
- file: .engineering/planning/story/ingress-observe-panel.md
  line: 51
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its three Rust acceptance tests have no file in scope; the only candidate is crates/spindle-cli/tests/observe.rs (inferred), which story:ingress-prometheus-metrics also claims and runs beside this story without saying so; name the test file and either order the two stories on it or give the panel its own test file"
```

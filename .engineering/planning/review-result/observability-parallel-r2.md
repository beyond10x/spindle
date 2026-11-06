---
format: aep.planning-md/3
id: review-result:observability-parallel-r2
kind: review-result
status: active
title: Parallel-safety critic, round 2, runtime observability
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

1. epic:runtime-observability — its stories table disagrees with the edges the revision added to the stories. Row 4 lists `external-answers-observed` as depending on "2, 3 (metrics)" only. The edge `external-answers-observed` → `ingress-observe-panel` exists because both edit `crates/spindle-ingress/src/observe.rs`. Row 2 omits the edges to the active `story:outcome-scoped-refusal` and to `story:fixed-caller-per-grant`. The paragraph below the table names only metrics, panel and topology as sharing `observe.rs`. The epic is still at revision 1, so a reader following it would work `external-answers-observed` beside the panel. Surface: cited (epic, lines 121-131; graph edges). Remedy: update the table and the overlap paragraph to match the edges, or drop the table's "depends on" column. — .engineering/planning/epic/runtime-observability.md:124,127,130

**What you read:** 7 set artifacts with `aep plan artifact show`, the full graph and `relations`, the r1 parallel result, and the active stories beside them. Those were `http-router-is-transport-free`, `outcome-scoped-refusal`, `fixed-caller-per-grant`, `refusal-payload-complete`, `one-test-binary-per-crate`, the kernel stories, and `browser-store` and `service-worker-serves-a-specification` for the browser-host epic. I also grepped the scopes of all 100 stories for the shared files, and checked `git branch --contains` for `unit/w10-caller` and `unit/w10-refusal`.

**Placement:**
- 6 stories placed from cited surfaces, 0 inferred-only, 0 unplaced. The epic has no scope of its own.
- 13 acceptance reads traced to a producer in the set. `depends_on` records 12 directly. The 13th is the metrics story's read of `families.json` from `observe-ingress-model`, ordered only through `ingress-request-log`, which is enough for waves and not a finding.

**Round 1 status:**
- Findings 1 and 3 hold fixed: `ingress-request-log` and `external-answers-observed` now have a `depends_on` edge to `story:fixed-caller-per-grant`, and both bodies name the files.
- Finding 2 holds fixed: `ingress-request-log` has an edge to `outcome-scoped-refusal`.
- Finding 4 is fixed on the edge (`external-answers-observed` → `ingress-observe-panel`, with the reason in its Order section). The epic still carries the old picture, which is the finding above.
- Finding 5 is fixed: the panel's Rust tests are in their own file, `crates/spindle-cli/tests/observe_panel.rs`.

**What I could not establish:**
- **`story:one-test-binary-per-crate`:** it is active and restructures `crates/spindle-cli/tests`, and no edge orders it against the observe stories. Only `ingress-request-log` and `ingress-observe-panel` carry the "under `tests/spindle-cli/` if it lands first" conditional. The metrics, external-answers and topology bodies cite `crates/spindle-cli/tests/observe.rs` without it. I left this out of the findings as in round 1, because the log orders them all after itself. Your call.
- **`story:refusal-payload-complete`:** it edits `answer.rs`, which the log lists as read-only, and the branch is already contained in `wave/10`. The store still shows it as active. I did not file it, since it has landed before these stories start.
- **Panel hand-off:** the panel's scope names `build.rs` and `observe.rs`, but not the cli source file where the embedded page is handed to the ingress as `&'static str`. It is also not in `Cargo.toml`, though `spindle-control` sets `build = "build.rs"` there. The metrics story edits `crates/spindle-cli/Cargo.toml` for its dev-dependency. I found no concurrent claimant of any such file, so this is an unplaced surface, not a collision.
- **Draft stories outside the set:** I did not assess draft stories that share files with the set (`one-ingress-several-specifications` on `serve.rs`, `periodic-bindings-retry-on-failure` on `host.rs`, `replay-with-decisions` on `cli.rs`, `decisions-rules-on-input-predicates` on `decisions.rs`, and others). The live-topology multi-specification test is written as `#[ignore]` pending the first of these and says so.
- **Out of my lane:** whether the panel's acceptance checks what `./metrics` returns goes to the acceptance critic.

```findings
- file: .engineering/planning/epic/runtime-observability.md
  line: 127
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the stories table lists external-answers-observed as depending on 2 and 3 (metrics) only, and ingress-request-log without the outcome-scoped-refusal and fixed-caller-per-grant edges; the overlap paragraph names only metrics, panel and topology on observe.rs; the revised stories add the external-answers-observed to ingress-observe-panel edge (shared observe.rs) and the two active-story edges, so the epic, still revision 1, schedules the pair the edges forbid; update the table and paragraph to match the edges"
```

---
format: aep.planning-md/3
id: review-result:observability-acceptance-r1
kind: review-result
status: active
title: Acceptance critic, round 1, runtime observability
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

- story:live-topology — the outcome says "An `invokes` edge has commands invoked", but no acceptance checks an `invokes` edge's number or its existence on a single specification; `emits_and_carries_edges_follow_the_counters` names only `emits` and `carries`, and the one `invokes` check is the `#[ignore]` multi-specification test, so this outcome can be left unbuilt while every active check passes — .engineering/planning/story/live-topology.md:75

**What I read:** 7 of 7 ids (the epic and 6 stories). I ran `aep plan artifact show` on each, plus `aep plan artifact kinds`, `aep plan artifact lifecycle story`, and `git grep` and `ls` in the tree for the Taskfile `check:*` names, `OBSERVE_ROOTS`, the telemetry catalogue and the proposal YAML.

**What I could not establish:**
- `ess/fixtures/observe/requests`, `metrics` and `topology` do not exist yet. The acceptances name `scripted.json`, `families.json` and `tasks.json` as the expected values, and story:observe-ingress-model drafts them. I cannot check their content until they exist.
- I did not run `ess specify validate` on `docs/design/proposals/observe-ingress.yaml`. The story claims it passes with toolchain 0.87.0.
- Possible out-of-lane points, which did not set the verdict:
  - The epic's outcome lists "view reads" among the metric families. The metrics story's outcome has no family of that name, and I could not tell whether `spindle_http_requests_total{kind="view"}` is meant to cover it (scope).
  - The panel banner shows five counts, while `RequestClass` has six variants, including `failed` (scope or design).
- The epic has an Outcome but no Acceptance section. I did not count that as a finding, because its stories carry the checks and the consumer's acceptance sentence is in its Origin.

```findings
- file: .engineering/planning/story/live-topology.md
  line: 75
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the outcome gives an `invokes` edge live numbers (commands invoked) but no active acceptance checks an `invokes` edge's existence or count on a single specification; `emits_and_carries_edges_follow_the_counters` names only emits and carries, and the only invokes check is the ignored multi-specification test"
```

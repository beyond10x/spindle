---
format: aep.planning-md/3
id: review-result:plan-scope-round-1
kind: review-result
status: active
title: Plan critic (scope), round 1
relations:
- reviews: story:spindle-admission-rules-are-declared
- reviews: story:spindle-placement-rules-are-declared
- reviews: story:rally-rules-are-declared
- reviews: story:telemetry-transport
- reviews: story:control-process-serves-the-api
- reviews: story:replay-ess-scenarios
- reviews: story:run-over-http-ingress
- reviews: story:rally-runs-as-three-processes
- reviews: story:rally-runs-in-three-microvms
- reviews: story:spindle-conforms-to-its-specification
- reviews: story:observe-ui
revision: 1
---
needs-revision
.engineering/planning/story/rally-rules-are-declared.md — the check `rally_synthesis_refusals_are_accounted_for` asks for every remaining `ESS-SYNTH-003` and `ESS-SYNTH-010` refusal to be gone or named, but the parent promises only its 23 findings and records those counts as the review's validation (Source), and the `ESS-SYNTH-010` part claims the same accounting as `binding_delivery_is_stated` — .engineering/planning/story/rally-rules-are-declared.md:63

**What I read.** 14 artifacts (the parent, 3 children, 9 other revised stories), plus epic:observe, epic:m0-run-in-process, epic:m1-m3-rally-in-three-vms, epic:deployment-targets and vision:spindle. Commands run:
- `aep plan artifact show` on each artifact
- `aep plan artifact graph`
- `git diff` of the store against `b9b0cd9`
- `git grep` on `b9b0cd9` for the UI embed, the firecracker test name and `ess/system.yaml`

I wrote the parent's promises down before reading the children. The parent has 24: 23 findings, each needing its own closing check, plus the umbrella acceptance that the three stories are implemented. All 24 trace to a child.
- **Split completeness.** The 23 findings, matched against the original parent block by file, line and message, are exactly 6 in admission (1, 2, 3, 4, 9, 13), 8 in placement (5, 6, 7, 8, 10, 11, 12, 14) and 9 in rally (15–23). Each finding has a named check, none is duplicated, and the warning and note counts match the parent's table (4/2, 3/5, 3/6).
- **Moved items.** Both landed with nothing lost.
  - The "embed the UI in the binary and serve it at `GET /`" outcome is now `the_ui_is_served_from_the_binary` in `story:control-process-serves-the-api`. It is absent from `main`, so observe-ui is right to drop it. `story:observe-ui` records the move.
  - The firecracker test `the_same_rally_placement_runs_on_qemu_and_on_firecracker` is absent from `main` (grep empty). It is now claimed by `story:rally-runs-in-three-microvms`, which carries the gap section, and `story:firecracker-launcher` records "Not delivered".
- **Other revised stories.** Each outcome traces to its parent epic's or the vision's text or to the story's own original outcome, and no two claim the same outcome. Only the `ESS-SYNTH-010` pair in the finding above is a duplicate.

**What I could not establish**
- `story:rally-rules-are-declared`'s finding 23 has `origin: pre-existing`; the original parent block had `introduced`. It may be a deliberate correction, but nothing says so.
- Out of my lane (acceptance), none of which set the verdict:
  - `story:control-process-serves-the-api` states 15-minute retention in its outcome with no acceptance check.
  - `story:firecracker-launcher` stays `implemented` while one acceptance is marked not delivered.
  - `story:spindle-conforms-to-its-specification` has `the_synthesis_refusals_are_listed`, which names `ESS-SYNTH-001`, the refusal placement finding 7 removes. It traces only loosely to "held by its conformance suite".
- Outside the set, none of which I counted as a gap:
  - `epic:deployment-targets` promises Docker, Compose, Kubernetes and a browser target, with no stories yet. Its operator order says "then … each its own story", so this is a stated deferral.
  - `epic:m1-m3-rally-in-three-vms` does not list `story:rally-is-watchable-in-a-terminal`, which decomposes it in the graph.

```findings
- file: .engineering/planning/story/rally-rules-are-declared.md
  line: 63
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the check `rally_synthesis_refusals_are_accounted_for` asks for every remaining ESS-SYNTH-003 and ESS-SYNTH-010 refusal to be gone or named, but the parent promises only its 23 findings and records those counts as the review's validation (Source), and the ESS-SYNTH-010 part claims the same accounting as `binding_delivery_is_stated`"
```

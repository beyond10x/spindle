---
format: aep.planning-md/3
id: review-result:plan-design-round-1
kind: review-result
status: active
title: Plan critic (design), round 1
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
story:control-process-serves-the-api — its acceptance `the_ui_is_served_from_the_binary` reads the `ui/dist` bundle that story:observe-ui builds, and no depends_on edge runs from it to story:observe-ui (the body only says the item "moved here") — .engineering/planning/story/control-process-serves-the-api.md:61, .engineering/planning/story/observe-ui.md (Outcome, "ui/ builds")
story:placement-guard — see the entry below
story:spindle-placement-rules-are-declared — its check `an_instance_of_a_refused_deployment_cannot_be_placed` reads the `Deployment` state that story:spindle-admission-rules-are-declared may reshape (`one_bundle_in_three_environments_is_decided` changes `Deployment` to one or three per bundle), and no depends_on edge records that order, so a depends_on edge to story:spindle-admission-rules-are-declared would say it — .engineering/planning/story/spindle-placement-rules-are-declared.md:37, .engineering/planning/story/spindle-admission-rules-are-declared.md:46
story:spindle-conforms-to-its-specification — its checks `the_suite_counts_are_held` and `the_synthesis_refusals_are_listed` read the `ess/` suite (the refusal count, the scenario count) that the placement and admission stories change (the `ESS-SYNTH-001` refusal is removed, guards add scenarios), and no depends_on edge says it runs after them — .engineering/planning/story/spindle-conforms-to-its-specification.md:38, .engineering/planning/story/spindle-placement-rules-are-declared.md:43
story:telemetry-transport — the sender is placed in "the instance side (`b10x-spindle run`)" while story:rally-runs-as-three-processes leaves the per-component entry point (a flag on `run` or a verb of its own) to the implementor, so the instance entry the control process launches may not start the sender, and no edge or body line settles which story wires it — .engineering/planning/story/telemetry-transport.md:74, .engineering/planning/story/rally-runs-as-three-processes.md:35

(Ignore the second line above: it is a slip. The valid findings are the four below it and the fenced block.)

Part 3. I read 12 artifacts in full (the parent, its 3 children, the 8 revised stories) with `aep plan artifact show`. I walked all 120 or so edges of `aep plan artifact graph`, including those outside the set. I found no cycle and no serialising chain. `aep plan artifact validate` reports valid. I traced about 14 acceptance reads to a producer in the set, and an edge records 11 of them. The three unrecorded are the UI bundle, the `Deployment` state and the suite counts, all listed above.

Part 4. What I could not establish:
- Whether the acceptance `a_match_between_three_processes_ends_with_a_winner` ("read from the table's view") needs the HTTP ingress of story:run-over-http-ingress. The body does not say how a view is read from a component process, so I raise no finding.
- Whether both findings-story seams around `spindle.md:110` (admission finding 2, placement finding 6) are one abstraction. They refuse different things, so I left them as they are.
- The shared edits to `docs/design/spindle.md` and the guest init belong to the parallel-safety critic.

```findings
- file: .engineering/planning/story/control-process-serves-the-api.md
  line: 61
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its acceptance `the_ui_is_served_from_the_binary` reads the `ui/dist` bundle that story:observe-ui builds, and no depends_on edge runs from it to story:observe-ui (the body only says the item moved here)"
- file: .engineering/planning/story/spindle-placement-rules-are-declared.md
  line: 37
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its check `an_instance_of_a_refused_deployment_cannot_be_placed` reads the Deployment state that story:spindle-admission-rules-are-declared may reshape (`one_bundle_in_three_environments_is_decided`), and no depends_on edge to story:spindle-admission-rules-are-declared records that order"
- file: .engineering/planning/story/spindle-conforms-to-its-specification.md
  line: 38
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its checks `the_suite_counts_are_held` and `the_synthesis_refusals_are_listed` read the ess/ suite that the placement and admission stories change, and no depends_on edge says it runs after them"
- file: .engineering/planning/story/telemetry-transport.md
  line: 74
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the sender is placed in `b10x-spindle run` while story:rally-runs-as-three-processes leaves the per-component entry point to the implementor, so the entry the control process launches may not start the sender, and no edge or body line settles which story wires it"
```

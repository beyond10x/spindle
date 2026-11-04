---
format: aep.planning-md/3
id: review-result:plan-scope-round-2
kind: review-result
status: active
title: Plan critic (scope), round 2
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
approve

All 24 promises in the parent (23 findings plus the umbrella acceptance) are still claimed by exactly one child. The round-1 finding is closed: `story:rally-rules-are-declared` no longer carries `rally_synthesis_refusals_are_accounted_for`, and it now says the 30-refusal count is "the review's record, not a promise of this story". `binding_delivery_is_stated` is the only owner of the `ESS-SYNTH-010` accounting. Finding 23 is back to `origin: introduced`.

What I checked for coverage and reach:
- **The split.** Admission still has 6 findings (4 warning, 2 note), placement 8 (3 warning, 5 note) and rally 9 (3 warning, 6 note), matching the parent's table. The new splits (finding 2 into two checks, finding 7 into `targets_and_requests_are_published_by_a_view` plus `retire_target_is_synthesised`) stay inside the finding that owns them.
- **The two moved items.** `the_ui_is_served_from_the_binary` is still claimed in `story:control-process-serves-the-api`. `the_same_rally_placement_runs_on_qemu_and_on_firecracker` is still claimed in `story:rally-runs-in-three-microvms`, with the gap recorded in both.
- **New acceptances.**
  - The instance-screen and view-screen UI checks in `story:control-process-serves-the-api` trace to the "instance and view screens" outcome in `story:observe-ui`, and that story records the carry-over.
  - The fifteen-minute retention check traces to the control story's own outcome.
  - The HTTP-ingress refusal check traces to the original Open note in `story:run-over-http-ingress`.
  - The telemetry check in `story:rally-runs-as-three-processes` is the half of the original telemetry-transport outcome ("Each instance sends…") that was handed over. `story:telemetry-transport` records the handover in its Scope section.

**What I read.** The parent, the three children, nine other revised stories and the parent epics and vision, using `aep plan artifact show`, `aep plan artifact graph` and `git diff` against `b9b0cd9`. I also grepped `docs/design/observe.md` for telemetry text.

**What I could not establish**
- No check says a real `b10x-spindle` instance in a QEMU or Firecracker microVM sends telemetry over vsock. `story:telemetry-transport` drives the sender from guest probes, `story:rally-runs-as-three-processes` tests only the Unix socket, and `story:rally-runs-in-three-microvms` only checks winners. The handover is recorded, but whether the guest init wires the sender is unstated. I left it out of the findings because I cannot tell whether that is a promise or an intended deferral.
- Outside the set and unchanged since round 1:
  - `epic:deployment-targets` has no stories for Docker, Compose, Kubernetes or the browser target. Its "then … each its own story" order makes this a stated deferral.
  - `epic:m1-m3-rally-in-three-vms` does not list `story:rally-is-watchable-in-a-terminal`, which decomposes it in the graph.

```findings
[]
```

---
format: aep.planning-md/3
id: review-result:plan-acceptance-round-1
kind: review-result
status: active
title: Plan critic (acceptance), round 1
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
story:spindle-admission-rules-are-declared — `refused_names_every_missing_capability` joins two independent outcomes with "and" ("`DeploymentRefused` carries the missing capabilities, and a refusal with an empty list is itself refused"), so one can pass while the other fails; split it into two checks — .engineering/planning/story/spindle-admission-rules-are-declared.md:39
story:spindle-placement-rules-are-declared — `system_declares_spindle_observe` already holds on `main` at `b9b0cd9` ("Closed by the check alone"), so it reads the same before and after the work and no work can close it; drop it from the acceptance or move it to a record of what already holds — .engineering/planning/story/spindle-placement-rules-are-declared.md:45
story:control-process-serves-the-api — the outcome promises "keeps the last 15 minutes per instance" (`docs/design/observe.md:110`) but no acceptance check observes retention, so the story can be closed with samples kept for any length of time; add a named check on the retention window — .engineering/planning/story/control-process-serves-the-api.md:37
story:run-over-http-ingress — `the_listener_binds_loopback_only` states two outcomes, "the listener's local address is `127.0.0.1`; an environment asking for a wildcard address is refused", and the bind and the refusal can pass or fail independently; split them — .engineering/planning/story/run-over-http-ingress.md:39
story:observe-ui — the outcome says the UI "shows the topology, instance and view screens; rally's `MatchState` is drawn as the board", but the two acceptance checks cover only the topology cards and the API types. `ui/src/screens/InstanceScreen.vue`, `ViewScreen.vue` and `ui/src/components/RallyBoard.vue` exist with no named test, so `implemented` is asserted rather than checked — .engineering/planning/story/observe-ui.md:40

What I read: 14 artifacts, all the ids you gave (the parent, its 3 children and the 8 revised stories). I ran `aep plan artifact show` on each, `aep plan artifact kinds` and `aep plan artifact lifecycle story`. I also checked `ess/system.yaml`, `Taskfile.yml` (`check:vm`, the fixture schema check), `examples/environments/*.toml`, the `ui/` tests and `docs/design/observe.md`.

What I could not establish:
- Whether `ess-conformance` at the pinned ESS tag has an authored-scenario reader, and which `ess-scenario` versions it admits. This bears on `replay-ess-scenarios` `a_scenario_in_another_format_is_refused`, whose "unless ESS's reader admits it" clause leaves its outcome open. I did not file it.
- Whether the `origin` values for the control, ingress and UI findings are introduced or pre-existing. I marked them `undecided`.
- Out of my lane: the `origin` labels on the child stories' embedded findings blocks, and whether the dependency edges among the revised stories are complete. These did not affect my verdict.

```findings
- file: .engineering/planning/story/spindle-admission-rules-are-declared.md
  line: 39
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "refused_names_every_missing_capability joins two independent outcomes with 'and' (DeploymentRefused carries the missing capabilities, and a refusal with an empty list is itself refused), so one can pass while the other fails; split it into two checks"
- file: .engineering/planning/story/spindle-placement-rules-are-declared.md
  line: 45
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "system_declares_spindle_observe already holds on main at b9b0cd9, so it reads the same before and after the work and no work can close it; drop it from the acceptance or record it as already held"
- file: .engineering/planning/story/control-process-serves-the-api.md
  line: 37
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: undecided
  message: "the outcome promises 'keeps the last 15 minutes per instance' but no acceptance check observes retention, so the story can be closed with samples kept for any length of time; add a named check on the retention window"
- file: .engineering/planning/story/run-over-http-ingress.md
  line: 39
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: undecided
  message: "the_listener_binds_loopback_only states two outcomes (local address is 127.0.0.1; an environment asking for a wildcard address is refused) that can pass or fail independently; split them"
- file: .engineering/planning/story/observe-ui.md
  line: 40
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: undecided
  message: "the outcome says the UI shows the topology, instance and view screens and draws MatchState as the board, but the acceptance names tests only for topology cards and API types; the instance screen, view screen and RallyBoard have no named check, so implemented is asserted not checked"
```

---
format: aep.planning-md/3
id: review-result:wave29-acc-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on wave 29
relations:
- reviews: story:binding-failure-escalates
- reviews: story:payload-field-through-conversion
- reviews: story:value-source-through-two-rows
- reviews: story:ess-pin-0-99-2
revision: 1
---
needs-revision

story:payload-field-through-conversion — the acceptance line "`conform_runs_an_outcome_with_a_conversion`: `conform` on the fixture; every executed scenario passes" already passes on 0.22.0, because a scenario that picks a conversion outcome hits the kernel's `Unsupported`. The target turns that into status `unsupported`, conform counts it as skipped rather than executed, and the other scenarios pass. The line needs to say that the conversion scenarios run (or name how many run) — .engineering/planning/story/payload-field-through-conversion.md:55 (mapping at crates/spindle-conformance/src/target.rs:349-352 and :358; "executed" leaves out `unsupported` at crates/spindle-cli/tests/spindle-cli/conform.rs:228-230)

story:binding-failure-escalates — the acceptance line "`conform_runs_an_escalating_binding`: `conform` on the fixture; every executed scenario passes" already passes on 0.22.0. Each scenario opens its host through `Host::admitted`, `Host::assemble` refuses the escalation as `HostError::Unsupported`, every scenario is skipped, and none is executed. The line needs to require that the escalation scenarios run — .engineering/planning/story/binding-failure-escalates.md:88 (refusal at crates/spindle-host/src/host.rs:1012-1017, mapping at crates/spindle-conformance/src/target.rs:189-196 and :358)

story:binding-failure-escalates — no acceptance line checks the refusal decided in D2 A, which refuses at start a binding whose receiving component does not declare the event in `publishes:`, naming both. If that refusal is never built, every listed test still passes — .engineering/planning/story/binding-failure-escalates.md:119-121

story:binding-failure-escalates — no acceptance line checks D4 A's "an optional field is left out". An implementation that reports `unmet` for a missing optional field passes `an_escalation_event_field_the_input_lacks_is_reported_unmet` — .engineering/planning/story/binding-failure-escalates.md:125-126

**What I read:** all 4 of 4 stories, with `aep plan artifact show` on each id, plus the wave page, `aep plan artifact kinds`, `aep plan artifact lifecycle story` and `aep plan artifact validate` (valid; it reports only older review-results). I checked the tree: conform's skip rule, the target's mapping of `Unsupported`, and `Host::assemble`. I checked ESS too: `git diff --stat 0.99.1 0.99.2`, the 0.99.2 release notes and CHANGELOG, and `checked_read` in 0.99.1.

**What I could not establish:**
- Whether `b10x-spindle conform` exits 0 when no scenario is executed. The two conform findings do not depend on it: "every executed scenario passes" holds whenever nothing executes.
- story:value-source-through-two-rows: in the absent-far-row case, ESS answers `unsupported` (`NoValue`), and so does 0.22.0. That part of `a_missing_row_in_the_chain_is_answered_as_ess_answers_it` passes before the work. The absent-`Optional` case still tells before from after, so I did not file it.
- Out of my lane (scope): story:value-source-through-two-rows names its missing-row test `a_missing_middle_row_is_refused_as_ess_refuses_it` in its Scope (line 82) but `a_missing_row_in_the_chain_is_answered_as_ess_answers_it` in its Acceptance (line 51). D3 A matches the Acceptance, so the Scope entry is stale.
- Public repository: no person, organisation, host or home path appears in the four story bodies. The wave page says "host operator", a role and not a name.

```findings
- file: .engineering/planning/story/payload-field-through-conversion.md
  line: 55
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the acceptance line \"`conform_runs_an_outcome_with_a_conversion`: `conform` on the fixture; every executed scenario passes\" already passes on 0.22.0, because a scenario that picks a conversion outcome hits the kernel's `Unsupported`, the target turns that into status `unsupported`, conform counts it as skipped rather than executed, and the other scenarios pass; the line needs to say that the conversion scenarios run (or name how many run)"
- file: .engineering/planning/story/binding-failure-escalates.md
  line: 88
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the acceptance line \"`conform_runs_an_escalating_binding`: `conform` on the fixture; every executed scenario passes\" already passes on 0.22.0, because each scenario opens its host through `Host::admitted`, `Host::assemble` refuses the escalation as `HostError::Unsupported`, every scenario is skipped and none is executed; the line needs to require that the escalation scenarios run"
- file: .engineering/planning/story/binding-failure-escalates.md
  line: 119
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "no acceptance line checks the refusal decided in D2 A, which refuses at start a binding whose receiving component does not declare the event in `publishes:`, naming both; if that refusal is never built, every listed test still passes"
- file: .engineering/planning/story/binding-failure-escalates.md
  line: 125
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "no acceptance line checks D4 A's \"an optional field is left out\"; an implementation that reports `unmet` for a missing optional field passes `an_escalation_event_field_the_input_lacks_is_reported_unmet`"
```

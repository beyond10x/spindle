---
format: aep.planning-md/3
id: review-result:wave29-sco-r1
kind: review-result
status: active
title: Plan critic (scope), round 1, on wave 29
relations:
- reviews: story:binding-failure-escalates
- reviews: story:payload-field-through-conversion
- reviews: story:value-source-through-two-rows
- reviews: story:ess-pin-0-99-2
revision: 1
---
needs-revision

story:binding-failure-escalates — The conform acceptance was weakened from "the escalation's scenario executes and passes" to "every executed scenario passes", and "Decided when scoped" does not record it, so a conform run that synthesises no escalation scenario now meets it. Either restore the original wording or add a decision saying conform may build no such scenario. — .engineering/planning/story/binding-failure-escalates.md:88 (against `git show 058698a9:.engineering/planning/story/binding-failure-escalates.md`:36-37)

story:payload-field-through-conversion — The conform acceptance was weakened from "the scenario of that outcome executes and passes" to "every executed scenario passes", and "Decided when scoped" does not record it, so a conform run with no conversion scenario now meets it. Either restore the original wording or add a decision saying conform may build no such scenario. — .engineering/planning/story/payload-field-through-conversion.md:55-56 (against `git show c4cc6c3b:.engineering/planning/story/payload-field-through-conversion.md`:33-34)

**What I read:** 4 stories, their revision-1 drafts as the parent (the consumer's four reported gaps), the wave page, the scoper reports and decisions in the session scratch directory, and ESS 0.99.1 `ir.rs` and `interpret/bindings.rs`. Commands: `git show c4cc6c3b/058698a9:<story>`, `cat` of each story, `aep plan artifact graph`, `aep plan artifact show story:related-via-nested-input`, and `grep` over `.engineering/planning`. I found 18 promises in the parent and traced 18 to an item. 16 are covered in full and 2 are narrowed without a record (the two findings above). No item reaches beyond its parent. No two items claim the same outcome. Nothing outside the set already claims these outcomes.

**What I could not establish:**
- **ESS pin, test scope:** acceptance now says "the tests of the crates that use ESS" where the draft said "the workspace's tests". I read this as matching the host rule against whole-workspace runs, not as a narrowing, but "Decided when scoped" does not say so.
- **Escalation, D2 A:** a binding whose receiving component does not declare the event in `publishes:` is refused at start. The scoper found no such check in ESS, so the consumer's own specification may still be refused. The decision is recorded, so this is not a silent narrowing, but whether the consumer's specification declares the event is not established.
- **Escalation, cascade:** ESS dispatches the escalation event on to further bindings (`interpret/bindings.rs`, end of `escalate`). No acceptance names delivery of the escalation event to a binding subscribed to it. Out of my lane (acceptance or design).
- **Conversion, source of a "stamped instant":** the scoper did not know where the consumer's stamped instant comes from (`generated`, `expr` or input). D1 A lifts the refusal for every source except `{response: ...}`, but the fixture tests only an input source. Out of my lane (acceptance).
- **Two rows, run and conform:** the outcome promises `run` and `conform`, but the acceptance has kernel tests only. The revision-1 draft was the same. The fixture also does not name a list-typed far field, which is the consumer's case. Out of my lane (acceptance).
- **Scope lists out of line with Acceptance:** in binding-failure-escalates, Scope does not place `a_universal_escalation_runs`, `an_escalation_event_field_the_input_lacks_is_reported_unmet` or `drop_retries_keeps_escalations`. In value-source-through-two-rows, Scope names `a_missing_middle_row_is_refused_as_ess_refuses_it` where Acceptance says `a_missing_row_in_the_chain_is_answered_as_ess_answers_it`, and does not place `a_chain_through_another_component_s_row_is_checked_at_every_hop`. Out of my lane (design).
- **Public repository:** no person, organisation, host or home path in the four story bodies or the wave page.

```findings
- file: .engineering/planning/story/binding-failure-escalates.md
  line: 88
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "The conform acceptance was weakened from \"the escalation's scenario executes and passes\" to \"every executed scenario passes\", and \"Decided when scoped\" does not record it, so a conform run that synthesises no escalation scenario now meets it. Either restore the original wording or add a decision saying conform may build no such scenario."
- file: .engineering/planning/story/payload-field-through-conversion.md
  line: 55
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "The conform acceptance was weakened from \"the scenario of that outcome executes and passes\" to \"every executed scenario passes\", and \"Decided when scoped\" does not record it, so a conform run with no conversion scenario now meets it. Either restore the original wording or add a decision saying conform may build no such scenario."
```

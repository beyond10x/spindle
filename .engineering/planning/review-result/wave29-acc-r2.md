---
format: aep.planning-md/3
id: review-result:wave29-acc-r2
kind: review-result
status: active
title: Plan critic (acceptance), round 2, on wave 29
relations:
- reviews: story:binding-failure-escalates
- reviews: story:payload-field-through-conversion
- reviews: story:value-source-through-two-rows
- reviews: story:ess-pin-0-99-2
revision: 1
---
**needs-revision**

All four round-1 findings are fixed. One new warning on the same story.

story:binding-failure-escalates — no acceptance line checks D3 A's "the event … is lost if the process stops before delivery; the docs say so", so every listed test still passes if the docs never state that limit — .engineering/planning/story/binding-failure-escalates.md:128-129

**Round-1 findings, each fixed:**
- **payload-field-through-conversion, the conform line:** it now requires every scenario that selects the conversion outcome to run, none skipped as `unsupported`, and says each was skipped on 0.22.0 (:160-162 in the `show` output).
- **binding-failure-escalates, the conform line:** fixed the same way (:65 in the `show` output; :88 in the file).
- **binding-failure-escalates, D2 A refusal:** now checked by `a_binding_escalating_to_an_event_the_component_does_not_publish_is_refused_at_start`.
- **binding-failure-escalates, D4 A optional field:** now checked by `an_optional_escalation_event_field_the_input_lacks_is_left_out`.

**What I read:** all 4 of 4 stories with `aep plan artifact show story:<id>`, plus:
- the wave page and `review-result:wave29-acc-r1`;
- `git diff 9572195b e0798f58` on the stories and the wave page;
- `aep plan artifact kinds` and `aep plan artifact validate` (valid; it reports only older review-results);
- `foreign_read` and the `value()` arms in `crates/spindle-kernel/src/supported.rs`;
- the skip rule in `crates/spindle-cli/tests/spindle-cli/conform.rs` (it already fails if nothing is executed);
- that `check:the_ess_pin_is_one_release` exists in `Taskfile.yml`.

**What I could not establish:**
- **The id:** the brief names `review-result:wave29-acceptance-r1`, but the store holds the round-1 record as `review-result:wave29-acc-r1`. I read that one.
- **value-source-through-two-rows, `a_chain_through_another_component_s_row_is_checked_at_every_hop`:** on 0.22.0 the chained read and the one-hop read of a foreign entity are both refused as unsupported, with different messages. If the test compares only the status, it passes on 0.22.0. It still fails if the middle-hop check is missing after the work, so I did not file it.
- **ess-pin-0-99-2, `a_trimmed_witness_on_the_deciding_element_is_answered_as_ess_answers_it`:** ESS's interpreter is unchanged in 0.99.2, so this line probably also passes on 0.99.1. It guards rather than shows the change. The quantifier line ("Its suite differs from 0.99.1's") carries the change.
- **binding-failure-escalates, `conform_runs_an_escalating_binding`:** it holds only because the fixture fills every escalation-event field from the command input. D4 A departs from ESS's placeholder exactly where a field is missing. That is the Scope's Fixture line, not the acceptance.
- **Out of my lane (scope):** value-source-through-two-rows D2 A (a `CHANGELOG.md` line and a status row) has no acceptance line either. I left it unfiled as housekeeping, not part of the outcome.
- **Public repository:** no person, organisation, host or home path appears in the four story bodies or the wave page. `b10x-spindle` is the product's binary name, and "host operator" and "a consumer" are roles.

```findings
- file: .engineering/planning/story/binding-failure-escalates.md
  line: 128
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "no acceptance line checks D3 A's \"the event … is lost if the process stops before delivery; the docs say so\", so every listed test still passes if the docs never state that limit"
```

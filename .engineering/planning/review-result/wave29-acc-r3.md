---
format: aep.planning-md/3
id: review-result:wave29-acc-r3
kind: review-result
status: active
title: Plan critic (acceptance), round 3, on wave 29
relations:
- reviews: story:binding-failure-escalates
- reviews: story:payload-field-through-conversion
- reviews: story:value-source-through-two-rows
- reviews: story:ess-pin-0-99-2
revision: 1
---
approve

All the earlier findings are resolved. Round 1's four findings stayed fixed and nothing reopened. Round 2's one finding is fixed: story:binding-failure-escalates now has an acceptance line for D3 A, `the_docs_say_an_escalation_is_not_stored` (`.engineering/planning/story/binding-failure-escalates.md:95-98`). Before the work it fails, because both documents say an escalating binding exits 1 (`docs/design/spindle.md:435`, `website/docs/reference/cli.md:99,119`). After the work it passes, once they say the event goes to the outbox, is not in the event history, and is lost if the process stops first. Reviewing the whole set again turned up no new acceptance defect.

**What I read:** all 4 of the 4 stories, plus:
- `aep plan artifact show` on each story and on `review-result:wave29-acc-r1` and `review-result:wave29-acc-r2`;
- the wave page, and `git diff 4f901853 492ae1ee` (only `binding-failure-escalates.md` changed);
- `aep plan artifact validate`: the store is valid; it only warns about older review results;
- the two documents the new docs test reads, and that `crates/spindle-channel/src/outbox.rs` exists;
- that `check:the_ess_pin_is_one_release`, `check:conformance` and `check:the_generated_types_have_no_drift` exist in `Taskfile.yml`;
- `checked_read` and `checked_concrete_read` in ESS 0.99.1, in the local ESS repository at tag `0.99.1`.

**What I could not establish:**
- **The ESS sources:** they are not in the cargo registry path the brief gives, because the crates come from git tags. I read the ESS repository at tag `0.99.1` instead.
- **The two "refused as ESS refuses it" lines can tell before from after.** ESS refuses a row read that breaks the target type as `Undetermined::Request` (`determined.rs:397-398`). 0.22.0 refuses it as `unsupported`. So the conversion line in story:payload-field-through-conversion fails on 0.22.0 if the test compares the kind of refusal and not just "refused". The same holds for the chained-read line in story:value-source-through-two-rows.
- **ess-pin-0-99-2, `a_trimmed_witness_on_the_deciding_element_is_answered_as_ess_answers_it`:** ESS's interpreter does not change in 0.99.2, so this line probably passes on 0.99.1 too. It guards rather than shows the change. The quantifier line ("Its suite differs from 0.99.1's") shows the change, so I did not file it.
- **Out of my lane (scope):** story:value-source-through-two-rows D2 A (a `CHANGELOG.md` line and a status row) still has no acceptance line.
- **Public repository:** no person, organisation, host or home path appears in the four story bodies or the wave page.

```findings
[]
```

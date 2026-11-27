---
format: aep.planning-md/3
id: review-result:wave29-des-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on wave 29
relations:
- reviews: story:binding-failure-escalates
- reviews: story:payload-field-through-conversion
- reviews: story:value-source-through-two-rows
- reviews: story:ess-pin-0-99-2
revision: 1
---
needs-revision

story:value-source-through-two-rows — Two stories each build half of one thing, row reads that carry a conversion, and nothing says how the halves join. payload-field-through-conversion admits a field that has a conversion from any source except a response, and adds a target check on related row reads. This story adds chained related reads. The body does not say whether a chained read through a conversion gets that check, and no depends_on edge runs from this story to story:payload-field-through-conversion, although U2 builds them in that order. — .engineering/planning/story/payload-field-through-conversion.md:81, .engineering/planning/story/value-source-through-two-rows.md:106, crates/spindle-kernel/src/supported.rs:839-843

What I read: the 4 stories in full (`aep plan artifact show`), the wave page, `aep plan artifact relations`, `graph`, `validate` and `waves`, and the code at `supported.rs:830-935` and `values.rs:270-290, 365-390, 478-530`.
- Edges: I walked 7 (4 `serves`, 3 `informed_by` to implemented stories outside the set) and followed the targets outside the set. There are no edges between the four stories and no cycle.
- Acceptance reads: I traced 1 to a producer in the set, U3's "tests of the crates that use ESS green on it" reading U1's and U2's new `conform` tests. No edge records it, and I don't count it as a finding. The suite is built from `ess/`, which neither U1 nor U2 touches. The merge order (U3 last) and the PR's CI run, which the wave's Exit section names, check the combined tree.
- Shape: no serialising chain, no horizontal slice. Each story can be shown working end to end on its own.
- `waves`: it already orders payload-field (wave 18) before value-source (wave 24) because their scope paths collide. So the missing edge does not cause a wrong order today. The finding is about the seam between the two stories.
- `validate` printed `valid`, plus warnings about old review-results that have no outcome recorded. Those are not mine to report.

What I could not establish:
- I did not check in ESS's source that ESS applies its conversion check to a chained read (`determined::checked_read` against `Reads::related`). The finding rests on the two story bodies and the code at `supported.rs:839`.
- Out of my lane (acceptance): the scope of value-source-through-two-rows names a test `a_missing_middle_row_is_refused_as_ess_refuses_it`, but its acceptance names `a_missing_row_in_the_chain_is_answered_as_ess_answers_it`.
- Out of my lane (parallel safety): U3 may move counts asserted in `crates/spindle-cli/tests/spindle-cli/conform.rs:332-410`, the file where U2 adds a case. The wave page records this.
- Public-repo check: grepping the 4 bodies and the wave page found no person, organisation, host or home path.

```findings
- file: .engineering/planning/story/value-source-through-two-rows.md
  line: 106
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "Two stories each build half of one thing, row reads that carry a conversion, and nothing says how the halves join. payload-field-through-conversion admits a field that has a conversion from any source except a response, and adds a target check on related row reads. This story adds chained related reads. The body does not say whether a chained read through a conversion gets that check, and no depends_on edge runs from this story to story:payload-field-through-conversion, although U2 builds them in that order."
```

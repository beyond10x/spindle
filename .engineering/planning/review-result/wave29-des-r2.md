---
format: aep.planning-md/3
id: review-result:wave29-des-r2
kind: review-result
status: active
title: Plan critic (design), round 2, on wave 29
relations:
- reviews: story:binding-failure-escalates
- reviews: story:payload-field-through-conversion
- reviews: story:value-source-through-two-rows
- reviews: story:ess-pin-0-99-2
revision: 1
---
approve

The round-1 finding is fixed. `story:value-source-through-two-rows` now has a `depends_on` edge to `story:payload-field-through-conversion` (frontmatter line 10). Its D4 A says a chained read through a conversion gets the same check as a one-hop row read, using the check the first story adds (lines 124-126). The new acceptance line `a_chained_read_through_a_conversion_is_checked_as_a_one_hop_read` tests that (lines 60-62), and the wave page states the edge (wave-29.md:70-71). ESS 0.99.1 does the same thing: `Reads::related` passes `target.conversion` to `checked_read` for the whole chain (`determined.rs:79-113` at rev d7833ee).

What I read: the 4 stories, the wave page and `review-result:wave29-des-r1` in full. I ran `aep plan artifact relations`, `graph`, `validate` and `waves`, and read the ESS 0.99.1 `determined.rs` lines 75-120.
- **Edges:** I walked 11: 4 `serves`, 3 `informed_by` and 4 `depends_on`. I followed the 3 `informed_by` targets outside the set; all are already implemented. There is no cycle.
- **Order:** U1 runs beside U2. Inside U2, payload-field comes before value-source. U3 depends on all three, and the wave page gives the reason. That is not a chain that forces the whole set into one queue.
- **Acceptance reads:** I traced 2 kinds of acceptance reads to a producer in the set, and an edge records each one:
  - U3's conform tests reading the U1 and U2 conform tests (3 edges).
  - value-source's chained-conversion line reading payload-field's conversion check.
- **Slices:** none is cut by layer. Each story can be shown working end to end on its own.
- **`validate`:** it printed `valid`, plus warnings about old review-results that have no outcome recorded. Those are not mine to report.
- **Public-repo check:** none of the bodies or the wave page names a person, organisation, host or home path.

What I could not establish:
- **Story text (no lane named, not a finding):** value-source's Scope gives the fixture as `<three-entity fixture>` (story line 98), but its frontmatter names it `related-chain`.
- **Out of my lane (scope):** payload-field-through-conversion says "Documents: none required" (line 73). U1 and value-source each add a status row. Whether payload-field should add one too is for the scope critic.

```findings
[]
```

---
format: aep.planning-md/3
id: review-result:wave29-par-r2
kind: review-result
status: active
title: Plan critic (parallel-safety), round 2, on wave 29
relations:
- reviews: story:binding-failure-escalates
- reviews: story:payload-field-through-conversion
- reviews: story:value-source-through-two-rows
- reviews: story:ess-pin-0-99-2
revision: 1
---
approve

**Round-1 finding: resolved.** `story:ess-pin-0-99-2` now has `depends_on` edges to all three other stories (`.engineering/planning/story/ess-pin-0-99-2.md:9-11`). Its acceptance names `conform_runs_an_escalating_binding`, `conform_runs_an_outcome_with_a_conversion` and `conform_runs_a_value_source_through_two_rows` (`:78-79`). D2 A (`:163-165`) and the wave page (`.engineering/waves/wave-29.md:67-71`) both say these run after U1 and U2 merge.

**What I read:** all 4 stories with their line numbers, the wave page and its diff from `9572195b` to `4f901853`, and `review-result:wave29-par-r1`. Commands: `aep plan artifact relations`, `aep plan artifact waves` (filtered to the set: 17 collision lines), `aep plan artifact validate` (`valid`), and `git grep` for escalation in the kernel and the conformance sources and for conversion in the host, the channel and `lines.rs`.
- Where each story lands: 4 placed from files the bodies cite (tests, fixtures and test registries inferred besides), 0 placed only by inference, 0 that could not be placed.
- Acceptance reads that depend on another story in the set: 4. Three are U3 running the conform tests of U1 and U2. One is `story:value-source-through-two-rows` (`:60-62`) reusing the conversion check that `story:payload-field-through-conversion` adds. `depends_on` edges record all 4.
- Derived waves put the four in order: escalates 2, payload 18, two-rows 30, ess-pin 31.
- Every file two units share is on the wave page's Surfaces list:
  - the conformance `main.rs` (U1, U2, U3)
  - `CHANGELOG.md`
  - `conform.rs` (U2, U3)
  - `status.rs` and the generated status pages (U1, U2)
  - `supported.rs` and `values.rs`, which only U2's two stories share, one after the other inside the unit
- The kernel has no `escalat` reference, so U1 does not touch `crates/spindle-kernel`, which the wave page gives to U2 only.
- Public-repository scan: the 4 story bodies and the wave page name no person, organisation, host or home path.

**What I could not establish:**
- Whether U1 adds a dependency that moves `Cargo.lock`. Not checked. U3 merges last after merging `wave/29`, so it would absorb that change.
- Not a finding: the wave page's `conform.rs` line (`wave-29.md:54-56`) names only U2's `conform_runs_an_outcome_with_a_conversion`. U2's second test, `conform_runs_a_value_source_through_two_rows`, adds to the same file. Both are in U2, and U2 is already ordered before U3, so this changes no schedule.
- Out of my lane: none seen.

```findings
[]
```

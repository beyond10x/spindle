---
format: aep.planning-md/3
id: review-result:wave29-par-r1
kind: review-result
status: active
title: Plan critic (parallel-safety), round 1, on wave 29
relations:
- reviews: story:binding-failure-escalates
- reviews: story:payload-field-through-conversion
- reviews: story:value-source-through-two-rows
- reviews: story:ess-pin-0-99-2
revision: 1
---
needs-revision

story:ess-pin-0-99-2 — Its acceptance requires the tests of the ESS-using crates to be green on 0.99.2. That set includes `conform_runs_an_escalating_binding` (U1) and `conform_runs_an_outcome_with_a_conversion` (U2), and both run on a suite ESS synthesises, which is the part 0.99.2 changes. Its body names neither test, and no depends_on edge runs from it to story:binding-failure-escalates or story:payload-field-through-conversion. So U3 can report green on a tree that lacks both tests. The wave page's merge-order reason, "its regenerated suite and generated types", covers only `conformance.toml` and the observe types. U1 and U2 change neither: `ess/` has no `escalate`, `conversions:` or `through` chain. — .engineering/planning/story/ess-pin-0-99-2.md:74, .engineering/planning/story/ess-pin-0-99-2.md:125, .engineering/planning/story/binding-failure-escalates.md:88, .engineering/planning/story/payload-field-through-conversion.md:55, .engineering/waves/wave-29.md:67

**What I read:** all 4 stories (`aep plan artifact show`), `aep plan artifact graph`, `aep plan artifact relations` and `aep plan artifact waves` (filtered to this set: 15 collision lines). I also read the wave page, `conformance.toml`, the `check:the_ess_pin_is_one_release` and `check:conformance` tasks, `conform.rs:325-412`, `status.rs:160-190`, and `git diff --stat 0.99.1 0.99.2` in the ESS repository.
- Surfaces: 4 placed from cited files (each has inferred test, fixture and registry paths besides), 0 placed only by inference, 0 unplaceable.
- Acceptance reads traced to a producer in the set: 2, both U3 reading tests that U1 and U2 add. A depends_on edge records 0 of them.
- Every shared file the store reports is on the wave page's Surfaces list. That covers the conformance `main.rs`, `conform.rs`, `CHANGELOG.md`, the status rows and generated pages, and `supported.rs`/`values.rs` inside U2, so none of those is a finding.
- U1's conditional collision with U3 (if ESS 0.99.2 changed `ResolvedRefusalAction`, `ResolvedBinding` or the interpreter's `escalate`) does not happen. The ESS diff touches only `crates/ui/*` and `ess-conformance/src/synthesize.rs`.
- The pin check reads only 4 files (`ess/ess-inputs.yaml`, `Taskfile.yml`, `.cargo/config.toml`, the CI workflow), so the new fixtures of U1 and U2 cannot break it.
- Public-repo scan: the 4 story bodies and the wave page name no person, organisation, host or home path. `b10x-spindle` is the binary's name.

**What I could not establish:**
- Whether the fixtures of U1 and U2 contain a refusal that quantifies over an input list, which is the only construct 0.99.2's synthesis change reaches. Both bodies leave the fixture contents open, so the finding above is a risk, not a proven break.
- Whether U1 adds a dependency that would move `Cargo.lock`. The wave page says U3 alone changes it; that is inferred and not checked.
- Out of my lane (design or acceptance): U2's second story walks a chain through `Sources::related` (`values.rs:482`). That is the same function where the first story adds the conversion target check on row reads. Neither acceptance says whether a chained read of a field with a conversion is checked.

```findings
- file: .engineering/planning/story/ess-pin-0-99-2.md
  line: 74
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its acceptance reads the green state of the ESS-using crates' tests, which story:binding-failure-escalates (conform_runs_an_escalating_binding) and story:payload-field-through-conversion (conform_runs_an_outcome_with_a_conversion) extend with synthesised-suite runs that 0.99.2's synthesis change can move, and neither its body names them nor a depends_on edge orders it after them, while the wave page's merge-order reason covers only conformance.toml and the observe types, which U1 and U2 do not change"
```

---
format: aep.planning-md/3
id: review-result:wave26-acc-r2
kind: review-result
status: active
title: Plan critic (acceptance), round 2, on wave 26
relations:
- reviews: story:related-selector-rows-quantifier
- reviews: story:guard-over-absent-row-field
- reviews: story:view-filter-parameter-literal
- reviews: story:run-fails-on-rejected-line
- reviews: story:clock-and-mint-in-every-mode
- reviews: story:scenario-act-without-outcome
- reviews: story:scenario-without-components
- reviews: story:release-linux-glibc-floor-and-musl
revision: 2
---
needs-revision

story:scenario-without-components — the Acceptance names three tests and none is the exit-3 refusal on an environment that lacks a capability. "Decided when scoped" adds that case ("A test names this."), so all three bullets can pass with it undone. The fix is a fourth bullet with its test name, the environment that lacks the capability, and the exit status 3 — .engineering/planning/story/scenario-without-components.md:21-28 (obligation at :126-128)

Round 1 acceptance findings, all resolved:
- **story:clock-and-mint-in-every-mode:** the one "honour" bullet is now eight single-outcome tests, each with an observable result. These cover the stdin and served mint, the line `at`, the served clock and the scenario clock. The scenario clock test passes with `holds` and exit 0 where the same document without `--clock` diverges. The two reset cases and the `seeded` binding are tests of their own, and the last one has a before and an after.
- **story:view-filter-parameter-literal:** the single "One test per form" bullet is now an enumerated list, F1a through F6. It adds the `the_consumer_filter_shapes_are_served` test and the rule that a form outside the list stays refused.
- **story:release-linux-glibc-floor-and-musl, acceptance part:** the Acceptance now has bullets for the aarch64 gnu archive on the 2.17 floor and for the `debian:bookworm` `--version` run of each gnu archive. It also covers the five-archive SHA256SUMS and SBOM count. The aarch64 row runs on `ubuntu-24.04-arm` (`release.yml:166`), so the bookworm run is possible there.
- **story:release-linux-glibc-floor-and-musl, privacy part:** a grep of all eight bodies for `/home`, the owner's name, `.local`, `@`, `.internal` and URLs finds none. "the worktree root" in `guard-over-absent-row-field` is a generic phrase, not a path. This one sits in the privacy lane and does not set my verdict.

What I read: 8 stories in full with `aep plan artifact show` (a loop over each id). I also read `review-result:wave26-acc-r1` and the wave 26 branch head `09a9dbf45`. I ran `git grep`-style checks of `.github/workflows/release.yml` (matrix, SBOM count) and `crates/spindle-conformance/src/target.rs` (clock use), plus a line lookup in the story file. I did not open the other three round-1 results; that is outside my lane.

What I could not establish:
- **View filter F2, parameter left out:** whether a test for F2 with the parameter left out compares rows. Per Not touched, both Spindle and the interpreter refuse that read, so the comparison is of the refusal. I treated this as covered by "compare with `interpret`" and did not raise it.
- **guard-over-absent-row-field, acceptance 1 and 2:** they are expected to pass at HEAD and so name no transition. "Decided when scoped" says so openly, and acceptance 3 carries the transition, so I did not raise it.
- **Out of my lane:** the `Outcome` of `story:guard-over-absent-row-field` still says the guard is "selected … instead of failing as undecidable", though Decided overrides it. The `Outcome` of `story:clock-and-mint-in-every-mode` is unchanged in how it handles periodic bindings. Whether `scenario-without-components` should name its fixtures is for scope.

```findings
- file: .engineering/planning/story/scenario-without-components.md
  line: 21
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Acceptance names three tests and none is the exit-3 refusal of a specification without components on an environment that lacks a capability, which Decided when scoped (:126-128) says a test names, so all three can pass with it undone"
```

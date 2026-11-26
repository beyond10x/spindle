---
format: aep.planning-md/3
id: review-result:wave26-acc-r4
kind: review-result
status: active
title: Plan critic (acceptance), round 4, on wave 26
relations:
- reviews: story:related-selector-rows-quantifier
- reviews: story:guard-over-absent-row-field
- reviews: story:view-filter-parameter-literal
- reviews: story:run-fails-on-rejected-line
- reviews: story:clock-and-mint-in-every-mode
- reviews: story:scenario-act-without-outcome
- reviews: story:scenario-without-components
- reviews: story:release-linux-glibc-floor-and-musl
revision: 1
---
needs-revision

story:clock-and-mint-in-every-mode — the parenthetical in `a_line_moves_the_clock`, "the guard compares with an instant after the day the test runs on", can never make the system clock select the other outcome. Any test run after 2026-02-01 has a system date later than February 1st, so a guard against an instant after that date answers the same on both clocks. A run that ignores `--clock` can still pass. The guard's instant has to lie between February 1st 2026 and the day the test runs on, so that the two clocks differ — .engineering/planning/story/clock-and-mint-in-every-mode.md:33-34

Earlier findings of my lane (`review-result:wave26-acc-r1` to `-r3`): all resolved.
- r3 `story:guard-over-absent-row-field`: bullet 3 now requires the refusal to suggest `defined(<field>)`.
- r3 `story:release-linux-glibc-floor-and-musl`: the Acceptance is split into "Read on the pull request's run" (`build` and `sbom` jobs) and "Read at the next tagged release" (`publish`).
- r3 `story:clock-and-mint-in-every-mode`: the contrast with the system clock is now stated in `a_line_moves_the_clock` and in `a_served_run_starts_on_the_clock_flag`. The new defect above is in the wording of that same contrast.
- r1 and r2: unchanged and still resolved.

What I read: all 8 stories in full with `aep plan artifact show story:<id>`, plus `review-result:wave26-acc-r1` to `-r3` and the stdin loop at `crates/spindle-cli/src/main.rs:690-780`. That code shows every rejection class in `story:run-fails-on-rejected-line` reaches the `failure` result, so one test line covers the class.

What I could not establish:
- Whether the clock guard in the story's fixture will use a date between February 1st 2026 and the test day. No fixture exists yet.
- `story:run-fails-on-rejected-line` states in its Outcome that an `already-there` seed entry is not rejected, and no acceptance bullet tests it. I did not raise it: the Outcome says it and the end-line count exists today.
- Out of my lane, not in the verdict: the Outcome of `story:guard-over-absent-row-field` still says the guard is "selected … instead of failing as undecidable", which its Decided section overrides. The Outcome of `story:clock-and-mint-in-every-mode` is unchanged on periodic bindings. The Outcome of `story:view-filter-parameter-literal` lists no F6 form. I found no person, company, host or home path in any body.

```findings
- file: .engineering/planning/story/clock-and-mint-in-every-mode.md
  line: 33
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "a_line_moves_the_clock says the guard compares with an instant after the day the test runs on, which makes the system clock and the February 1st clock select the same outcome (any test day is after 2026-02-01), so the stated contrast with the run without --clock cannot hold and a run that ignores --clock can pass; the instant must lie between February 1st 2026 and the test day"
```

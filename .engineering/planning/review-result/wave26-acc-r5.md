---
format: aep.planning-md/3
id: review-result:wave26-acc-r5
kind: review-result
status: active
title: Plan critic (acceptance), round 5, on wave 26
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
approve

Round 4's one finding is resolved, and nothing new in the acceptance lane would change what was drafted.

- **`story:clock-and-mint-in-every-mode`, `a_line_moves_the_clock`:** the guard is now fixed as `now < 2026-06-01T00:00:00Z`. It is true on February 1st and false on any system clock after 2026-06-01, so a run that ignores `--clock` cannot pass. The same contrast holds for `a_served_run_starts_on_the_clock_flag` and for the scenario bullet (`holds` and exit 0, against exit 1 without `--clock`). Citation: `.engineering/planning/story/clock-and-mint-in-every-mode.md:28-44`.
- **Rounds 1 to 3:** all still resolved. The bullets now name observable results, F1a to F6 are enumerated, and the aarch64 and `debian:bookworm` checks are present. The exit-3 refusal bullet is in `story:scenario-without-components`, `defined(<field>)` is required in `story:guard-over-absent-row-field`, and the release acceptance is split into what is read on the PR and what is read at the next tag.
- **Privacy:** a grep of all eight bodies for `/home`, the owner's name, `.local`, `@`, URLs and `worktree` found nothing identifying. The one hit is the generic phrase "worktree root" in `story:guard-over-absent-row-field`.

What I read: 8 stories in full with `aep plan artifact show story:<id>`, namely related-selector-rows-quantifier, guard-over-absent-row-field, view-filter-parameter-literal, run-fails-on-rejected-line, clock-and-mint-in-every-mode, scenario-act-without-outcome, scenario-without-components and release-linux-glibc-floor-and-musl. I also read `.engineering/waves/wave-26.md` and `review-result:wave26-acc-r1` to `-r4`. The branch head is `364ab3fac`.

What I could not establish:
- **Clock control run:** in `a_line_moves_the_clock`, "the same command on a run without `--clock`" most plausibly means the command without its `at` line. An `at` of February 1st on the system clock would be rejected as earlier, not answer the other outcome. I treated this as wording, not a finding.
- **`already-there` seed entry:** the `run-fails-on-rejected-line` Outcome says it is not rejected, and no acceptance bullet tests it. The end-line count exists today, so I did not raise it.
- **Parity tests that may already pass:** `guard-over-absent-row-field` acceptance 1 and 2 are expected to pass at the start, which the story's Decided section says openly. Acceptance 3 carries the transition.
- **Out of my lane, not in the verdict:**
  - The Outcome of `story:guard-over-absent-row-field` (`:14-16`) still reads as if the guard is selected, which "Decided when scoped" overrides.
  - The Outcome of `story:clock-and-mint-in-every-mode` is unchanged on periodic bindings.
  - The Outcome of `story:view-filter-parameter-literal` lists no F6 form.
  - The scenario-act Outcome wording is consistent with its Decided section.

```findings
[]
```

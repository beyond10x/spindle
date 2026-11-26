---
format: aep.planning-md/3
id: review-result:wave26-acc-r3
kind: review-result
status: active
title: Plan critic (acceptance), round 3, on wave 26
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

story:guard-over-absent-row-field — the third acceptance bullet requires the refusal to name "the field and the guard", but not to suggest `defined(<field>)`. The Outcome (:18) and "Decided when scoped" (:108) both require the suggestion, so the bullet can pass without it. The bullet should say the refusal also suggests `defined(<field>)` — .engineering/planning/story/guard-over-absent-row-field.md:33 (obligation at :18 and :108)

story:release-linux-glibc-floor-and-musl — bullets 1, 3 and 5 are about assets uploaded and `SHA256SUMS`, but the `publish` job that writes them is skipped on `pull_request` (`release.yml:369`). Bullet 6 says the PR's run "shows each check", and a PR run cannot show an upload or `SHA256SUMS`. The acceptance should say where each is read on the PR (the `build` job's archive artifacts, the `sbom` count check) and which part is read only at the next tagged release — .engineering/planning/story/release-linux-glibc-floor-and-musl.md:29,33,37-38 (`.github/workflows/release.yml:367-369`)

story:clock-and-mint-in-every-mode — `a_line_moves_the_clock` and `a_served_run_starts_on_the_clock_flag` accept "the outcome that guard selects on February 1st" without requiring that the guard selects a different outcome on the system clock. A run that accepts `--clock` and ignores it can therefore pass. Only the scenario bullet states the contrast ("the same document without `--clock` exits 1") — .engineering/planning/story/clock-and-mint-in-every-mode.md:31,38

Earlier findings of my lane:
- **Round 1:** all four are resolved, as `review-result:wave26-acc-r2` found.
- **Round 2:** the `story:scenario-without-components` finding is resolved. The fourth bullet, `a_specification_without_components_is_refused_on_a_lacking_environment`, now names the environment lacking `store.durable` and exit 3.

What I read: all 8 stories in full with `aep plan artifact show`, plus `review-result:wave26-acc-r1` and `-r2`. I checked these in the tree:
- the `release.yml` triggers and the `publish` and `gate` `if:` conditions;
- `main.rs:515-535`, which refuses a non-uuid-v4 mint binding today, so the `seeded` acceptance has a before state;
- the `host.rs` mint refusal.

A grep of the eight bodies found no person, company, host or home path.

What I could not establish:
- Whether the sibling repositories cited as "a sibling repository" are public. They are anonymised in the bodies, so there is nothing to leak.
- Out of my lane, not in the verdict:
  - The `Outcome` of `story:guard-over-absent-row-field` (:14-16) still says the guard is "selected … instead of failing as undecidable", which "Decided when scoped" overrides.
  - The `Outcome` of `story:clock-and-mint-in-every-mode` is unchanged in how it handles periodic bindings.
  - The `Outcome` of `story:view-filter-parameter-literal` lists only parameter forms, while "Decided when scoped" adds F6 (a row field under a case fold).

```findings
- file: .engineering/planning/story/guard-over-absent-row-field.md
  line: 33
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the third acceptance bullet requires the refusal to name the field and the guard but not to suggest defined(<field>), which the Outcome (:18) and Decided when scoped (:108) require, so it can pass with the suggestion absent"
- file: .engineering/planning/story/release-linux-glibc-floor-and-musl.md
  line: 29
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the upload and SHA256SUMS bullets (:29,:33,:37) cannot be observed on the pull request run that :38 names as the check, because the publish job is skipped on pull_request (release.yml:369); the acceptance does not say where each is read on the PR or that it is read at the next tagged release"
- file: .engineering/planning/story/clock-and-mint-in-every-mode.md
  line: 31
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "a_line_moves_the_clock (:31) and a_served_run_starts_on_the_clock_flag (:38) accept the outcome selected on February 1st without requiring the guard to select a different outcome on the system clock, so a run that ignores the clock can pass; only the scenario bullet states the contrast"
```

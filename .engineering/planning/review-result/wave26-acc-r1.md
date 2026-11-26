---
format: aep.planning-md/3
id: review-result:wave26-acc-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on wave 26
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

story:clock-and-mint-in-every-mode — the acceptance `the_served_run_and_the_scenario_take_both_flags` says HTTP and `--scenario` runs "honour" both flags but names nothing to observe, and for `--scenario` the story's own Decided section says ESS's runner keeps its own clock and ids, so no output a test could compare is identified. The bullet also bundles four checks: the HTTP clock, the HTTP mint, the scenario clock and mint, and the reset override. — .engineering/planning/story/clock-and-mint-in-every-mode.md:34 (Decided at :68-69)

story:view-filter-parameter-literal — "One test per form above" is ambiguous. Each Outcome bullet holds two or three forms: literals of three types; `in_ignore_case`, `equals_ignore_case` on a list element, and `trimmed(...)`. One test per bullet can pass while a sub-form is still refused. Decided then counts the three added forms one test each, so the unit of "form" differs within the section. — .engineering/planning/story/view-filter-parameter-literal.md:38

story:release-linux-glibc-floor-and-musl — the Acceptance section omits two obligations that "Joined" and "Decided when scoped" add: the `aarch64-unknown-linux-gnu` archive on the 2.17 floor with the `objdump -T` check, and the `--version` run of each Linux archive in `debian:bookworm`. All four acceptance bullets can pass with these undone. — .engineering/planning/story/release-linux-glibc-floor-and-musl.md:26-36 (obligations at :38-44 and :128-131)

story:release-linux-glibc-floor-and-musl — the Scope cites a local worktree directory name, `a local worktree path in a public repository. The branch and path should be cited in repository terms or dropped. Sibling-repository paths (a sibling repository, a sibling repository, a sibling repository) are cited the same way; I could not tell whether those repositories are public. — .engineering/planning/story/release-linux-glibc-floor-and-musl.md:107

What I read: all 8 story ids in full with `aep plan artifact show` (related-selector-rows-quantifier, guard-over-absent-row-field, view-filter-parameter-literal, run-fails-on-rejected-line, clock-and-mint-in-every-mode, scenario-act-without-outcome, scenario-without-components, release-linux-glibc-floor-and-musl), plus `.engineering/waves/wave-26.md`, `aep plan artifact kinds` and `aep plan artifact lifecycle story`. I also checked `crates/spindle-cli/src/main.rs:690-775` and the `release.yml` triggers; `release.yml` runs on `pull_request` for its own path, so the release acceptance can be observed in the PR's CI run.

What I could not establish:
- Outside my lane, left out of the verdict: story:scenario-act-without-outcome has an Outcome that still says "not holding … exits 1". Its Decided section says exit 0. I treated Decided as overriding, as the brief says. Its Acceptance test 1 ("exits as ESS's runner exits") is consistent with Decided, but the Outcome should be restated.
- Also outside my lane: story:clock-and-mint-in-every-mode keeps the periodic-binding sentence in its Outcome, although Decided says no `run` mode fires them.
- Also outside my lane: story:view-filter-parameter-literal cites "gap-file section ordinals" for a file that is not in the repository.
- The wave 26 Exit is `implemented` on a `test_result`. I did not check whether the guard-over-absent-row-field parity tests (acceptance 1 and 2, expected to pass at HEAD) can fail before the work.
- The person, company and host-path check found nothing else. Consumers are anonymous, and `beyond10x` already appears in the repository's `Cargo.toml` and `README.md`.

```findings
- file: .engineering/planning/story/clock-and-mint-in-every-mode.md
  line: 34
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the acceptance says HTTP and --scenario runs 'honour' --mint-seed and --clock but names no observable result, and for --scenario the story's Decided section says ESS's runner keeps its own clock and ids; the bullet also bundles the HTTP clock, the HTTP mint, the scenario flags and the reset override"
- file: .engineering/planning/story/view-filter-parameter-literal.md
  line: 38
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "'One test per form above' is ambiguous because each Outcome bullet holds two or three forms (three literal types; in_ignore_case, equals_ignore_case on a list element and trimmed), so one test per bullet can pass while a sub-form stays refused"
- file: .engineering/planning/story/release-linux-glibc-floor-and-musl.md
  line: 26
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Acceptance section omits the aarch64 gnu 2.17 floor check and the debian:bookworm --version run of each Linux archive that the Joined and Decided sections add, so all four bullets can pass with them undone"
- file: .engineering/planning/story/release-linux-glibc-floor-and-musl.md
  line: 107
  category: privacy
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Scope cites a local worktree directory name (a local worktree path in a public repository; cite repository terms or drop it"
```

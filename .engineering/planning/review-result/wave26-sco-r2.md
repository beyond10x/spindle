---
format: aep.planning-md/3
id: review-result:wave26-sco-r2
kind: review-result
status: active
title: Plan critic (scope), round 2, on wave 26
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

- story:run-fails-on-rejected-line — Outcome ("Named …") — the "Named 2026-11-26" paragraph cites the consumer's "gap 10g" as the source for stdin-line and seed-entry exit status, but 10g in the report is about `--scenario` ("`run` exits 0 after `--scenario` refusals in some paths"). It also cites "a consumer's first list of gaps", which is in none of the reports I was given. No report states a stdin or seed exit-status gap. The round-1 fix is only partly done. The paragraph should either say the operator's order is the origin of the stdin and seed parts, or cite the report that holds them. It should also say 10g's `--scenario` exit code is covered by `story:scenario-act-without-outcome` (exit stays ESS's runner's verdict) — `.engineering/planning/story/run-fails-on-rejected-line.md:25`; the 10g quote is at `<local path>`

**Round 1 findings, my lane (`review-result:wave26-sco-r1`)**
- "Seven more routes" in `story:view-filter-parameter-literal`: resolved. It now says nine, which matches the nine 0-journey view routes in the view-filter report. The 205 journeys also sum over the nine routes that have journeys.
- Origin of `story:run-fails-on-rejected-line`: not resolved, see the finding above.
- Seeded `identity.mint` binding and unchecked `clock` binding in `story:clock-and-mint-in-every-mode`: resolved. The `seeded` binding is served (`a_seeded_mint_binding_is_served`) and the `clock` binding is listed under Not covered.
- Scenario and HTTP clock stepping, and gap 10f (`at:`), in `story:clock-and-mint-in-every-mode`: resolved. All are listed under Not covered.
- Error, events and continue-after-divergence in `story:scenario-act-without-outcome`: resolved. The error is claimed (`a_refused_act_prints_its_error`). Events and continue-after-divergence are under Not covered, with the stop at the first divergence.

**Other checks**
- No person, company, host or home path appears in any of the 8 story bodies or in the wave page. The release story is generic: "sibling repository", "the organisation".
- No two stories claim the same outcome.
- No story reaches beyond the reports.

**What I read:** 8 stories and the wave page, the two consumer reports, `JOINED-OPEN.md`, `review-result:wave26-sco-r1`, and the diff of `d7f224007`. The commands were `aep plan artifact show` for each story and `review-result:wave26-sco-r1`, `aep plan artifact graph`, and `git show --stat`, `git diff` and `grep` in the worktree.
- Promises extracted: 28. These are the 10 numbered gaps of the simulation report, gap 10 split into 10a to 10g, plus 18 view-filter routes. The two non-view ingress refusals are counted as 2 more, wave 27 drafts.
- Promises traced to a story: 26. Gaps 1, 3 to 9a and the 18 view routes are claimed by wave-26 stories. Gap 2 is archived as not a gap. Gaps 9b, 10b, 10c and the two ingress refusals go to wave 27. Gaps 10a and 10d to 10f are recorded as not gaps or not stories. Gap 10g is in the finding.

**What I could not establish**
- Whether a consumer's "first list of gaps" exists with a stdin exit-status gap. It is in none of the reports I was given.
- Whether the gap-10a decision (the 1000-step cap, "no story") is recorded in the store. I saw it only in `JOINED-OPEN.md`, which is scratch.

**Out of my lane (not counted in the verdict)**
- `story:release-linux-glibc-floor-and-musl`: "Decided when scoped" points to a "Not covered" section that does not exist in the body.
- The same story's Outcome says the gnu binary comes "beside today's archives", while "Decided when scoped" says the gnu asset name stays.
- `story:clock-and-mint-in-every-mode`: HTTP `--clock` and `--mint-seed`, with plain-reset fallback to the seed, go beyond the stdin and scenario clock and mint asked for in gaps 6 and 7. I did not count this as reach, because the title "every run mode" and the order "resolve all" cover it.

```findings
- file: .engineering/planning/story/run-fails-on-rejected-line.md
  line: 25
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome cites the consumer's gap 10g, which is about --scenario exit codes, and 'a consumer's first list of gaps', which is in no listed report, as the source of the stdin and seed exit-status change; it should say the operator's order is the origin of those parts or cite the report that holds them, and say 10g's --scenario exit code is covered by story:scenario-act-without-outcome"
```

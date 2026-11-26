---
format: aep.planning-md/3
id: review-result:wave26-sco-r3
kind: review-result
status: active
title: Plan critic (scope), round 3, on wave 26
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
approve

Round 2's one scope finding is resolved, and the set has no finding in my lane this round.

- **Round 2 finding on `story:run-fails-on-rejected-line` (Outcome, "Named 2026-11-26"):** resolved.
  - The paragraph no longer cites gap 10g or a "first list of gaps". It now names the operator's order and the consumer's 2026-11-26 message as the origin, and says the consumer's written report does not hold the item.
  - It hands the `--scenario` exit-code item to `story:scenario-act-without-outcome`, where that story's Outcome keeps the exit as ESS's runner's verdict.
- **Round 1 findings:** still resolved.
  - `story:view-filter-parameter-literal` says "nine more routes". The report lists nine 0-journey routes and nine routes with journeys, which sum to 205.
  - `story:clock-and-mint-in-every-mode` has `a_seeded_mint_binding_is_served`, and its Not covered section lists the `clock` binding, scenario and HTTP clock stepping, and the scenario `at:`.
  - `story:scenario-act-without-outcome` claims the printed error (`a_refused_act_prints_its_error`) and lists events and continue-after-divergence under Not covered.
- **Coverage, both directions:**
  - There is no epic parent, so I took the promises from the two reports, the operator's item 4 (a run exits 0 when a seed line is rejected), the release order and the glibc floor finding. That is 39 promises, and I traced all 39.
  - 29 are claimed by a wave-26 story: gaps 1, 3, 4, 5 (the message), 6, 7, 8, 9a and 10g, the 18 filtered views, item 4 and the release.
  - 5 are recorded decisions or Not covered entries:
    - gap 2, archived with its reason in `story:unknown-instance-after-exists-false-guard`;
    - gaps 10d and 10e, not gaps;
    - gap 10f, listed in Not covered of the clock story;
    - gap 10a, the 1000-step cap, "no story" in `JOINED-OPEN.md`.
  - 5 go to wave 27: gaps 9b, 10b and 10c, and the two ingress refusals (a caller attribute no HTTP caller proves, and input fields in the query string).
  - No two stories claim one outcome. Each filtered view's refused leaf traces to a form F1 to F6, or to a leaf already read.
  - No story lands in something the reports or the order did not ask for.
- **Public repository:** no person, company, host or home path appears in the 8 story bodies or in `.engineering/waves/wave-26.md`.

**What I read:** 8 stories and the wave page with `aep plan artifact show` and `aep plan artifact graph`, plus `review-result:wave26-sco-r1` and `-r2` and `story:unknown-instance-after-exists-false-guard`. I also read the consumer reports, `JOINED-OPEN.md` and the critic brief, and grepped the story bodies for names and paths.

**What I could not establish:**
- Whether the consumer's 2026-11-26 message, which I cannot see, asks for `--mint-seed` and `--clock` on a served run. The two reports ask for them on stdin and scenario runs; the clock story's served-run acceptances rest on the story title "every run mode" and the order "resolve all". I did not count this as reach.
- Whether the gap 10a decision (no story for the 1000-step cap) and the two ingress refusals (the wave-27 drafts "to write") are recorded anywhere in the store. They are named only in `JOINED-OPEN.md`, and the wave page's Selection does not name them.
- Out of my lane, and not setting the verdict: the release story's Scope names "the release-gates repository", an internal-looking repository name, in a public body. The design critic or the owner may want to generalise it.

```findings
[]
```

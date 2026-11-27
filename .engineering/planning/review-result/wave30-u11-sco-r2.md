---
format: aep.planning-md/3
id: review-result:wave30-u11-sco-r2
kind: review-result
status: active
title: Plan critic (scope), round 2, on wave 30 (U11)
relations:
- reviews: story:affects-over-input-list
revision: 1
---
approve

I read U11 (`story:affects-over-input-list`, revision 11) in full with `aep plan artifact show`, plus the head commit diff (`git show HEAD`), `review-result:wave30-u11-sco-r1`, the wave page's U11 lines (`.engineering/waves/wave-30.md:31-32, 52, 68-79, 89, 97-102`) and `aep plan artifact graph`. I also checked `supported.rs:600-640` and the `swept` outcome in `lot.yaml:84-100`, and ran `git grep` for `compensates` in the stories on `plan/wave-29` and `plan/wave-30`.

**Round-1 finding: fixed.** The Acceptance now has `an_affects_entry_over_an_input_list_deletes_only_held_rows`, so the `deletes:` half of "a `moves:` or `deletes:` entry acts only on a held row" is covered.

**Promises:** I took 15 from the parent: Outcome `:39-55` plus Consumer gap `:93-97`. All 15 are covered by an Acceptance or Scope line.
- **Promises listed:** per-element in list order; sets updates a held row; sets creates in `initial`; moves acts only on held rows; deletes acts only on held rows; the element filter skips; an element naming the subject is skipped; `repeated: last_wins`; parity with ESS; `admit`, stdin and HTTP each answer; both refusal sites go; the two other refusals stay; the consumer's `each: {in: input.contracts, ...}` is no longer refused.
- **Nothing beyond the parent:**
  - The new `affects-each-network` fixture is there for the HTTP promise.
  - The `compensates: true` outcome keeps the existing refusal test running (D1).
  - The docs line follows from removing the refusal.
- **No duplicate claims:** the graph shows no other item claiming this outcome, and no story on wave 29 or 30 starts serving `compensates`.

**Could not establish:**
- **Stale Scope lines (design or acceptance lane, not mine):**
  - `.engineering/planning/story/affects-over-input-list.md:112` still names the merged test `run_answers_an_affects_entry_over_an_input_list_on_stdin_and_http` in `outcome_scoped_refusal.rs`. The Acceptance has split it into `stdin_runs_…` and `http_serves_…`, and the HTTP one runs on the new fixture.
  - `:111` (Tests (new)) leaves out the new deletes test.
- **Kept refusals untested:** no current test in `crates/*/tests` checks the messages of the two refusals that stay ("rows quantifier in an `affects:` filter", "beside no subject"). The Outcome's "stay" (`:53`) is covered only by the Scope line. Whether that needs a guard test is for the acceptance critic.

```findings
[]
```

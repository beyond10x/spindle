# R-03 report review

Reviewer 2 (report reviewer), from `references/review-agents.md` § 2. Inputs: `report.md`,
`round.json`, `shots/`, the raw run (`.out/run/`: `pages/`, `measured/`, `plays/`, `verdicts/`;
`.out/measured.json`, `.out/gates.json`, `.out/test-results/*/error-context.md`), the measure run's
Playwright log (kept outside the repository), `../R-02/` (`report.md`, `after/round.json`,
`triage.md`, `review-report.md`), `../trend.md`, `baseline.json`, `known-failing.json`,
`findings/`, `scenarios/`, the source of `cli/`, `core/`, `e2e/` and the round task in the
repository's `Taskfile.yml`. Recomputed with a scratch script that imports nothing from the project.
Ratchet: `node cli/ratchet.ts` (read-only, no `--update`). Nothing paid or live ran; no Playwright,
no build. Not available: operator notes (`R-03/README.md` does not exist), recordings,
questionnaires (no people).

**Verdict: not fit to send as is.** Every number recomputes to the report's value. But the report
does not say that the run is red: 9 of 220 verdicts fail with no mark and no finding (the 4 plays
of `request-log-size` by pointer, both of its keyboard plays at 1280x800, and 3 of the 4 `release`
keyboard plays), and a tenth test, the regression asset of the closed UXF-018, is red and
appears nowhere in the report. The ratchet fails with 27 violations. The base moved from R-02's
82faeb87 to 8591d014 (57 commits to `website/docs`), so the page keys are not comparable with the
baseline. And the measured site is no longer in the tree: `website/docs/concepts/observe.md` and
`website/docs/getting-started.md` were edited after the build, and those edits target the two
regressions this run shows. **Default action:** do not send R-03 and do not take any of its values
into the baseline. Freeze a build of the current tree (`.out/R-03-build`), re-run the measure
against it with an `R-03/README.md` that names the base and the edits, and write the report again
with § 4 of this review. Take the page `reading_load.*` keys in under a base decision id, as
`R-01-base-82faeb87` did. File findings for the `request-log-size` regression (§ 2.1) and the
`release` keyboard failures (§ 2.2) if the re-measure still shows them.

## § 1 Are the numbers right and meaningful?

### Recomputed from the raw run

| number | report | recomputed | diff |
|---|---|---|---|
| page checks / measured-only / plays / verdicts | 52 / 78 / 64 / 220 | 52 / 78 / 64 / 220 | 0 |
| red as run | 9 of 220 | 9: 4 gate-4 + 5 gate-5-keyboard | 0 |
| gate-1 / gate-2 / gate-5 | 52/0/0, 30/0/22, 52/0/0 | same; the 22 known are "docs sidebar" at 1280x800 (UXF-008) | 0 |
| gate-4 / gate-5-keyboard | 28/4/0, 25/5/2 | same | 0 |
| task keys, 8 tasks × 6 keys (report, `round.json`, `.out/measured.json`) | table | identical | 0 |
| per-play cells (reached, actions, looks, tabs, detours), 64 plays | table | identical | 0 |
| page keys, 13 pages × 8 keys | table | identical | 0 |
| reading load: median words, long paragraphs | 1375, 23 | 1375, 23 | 0 |
| trend row: actions, looks, axe, long, tasks, glance max, shift max | 79, 180, 0, 23, 8, 3, 0 | same | 0 |
| worst pages and failure counts | reference-cli, 1 / 1 / 0 / 0 | same | 0 |
| Playwright tests | not reported | 225: 10 failed, 215 passed | the 10th failure (UXF-018) is missing |

**No number differs.** The numbers are not all the run produced: the measure run also ran
`e2e/findings.spec.ts` and `e2e/break.spec.ts` (33 tests), and the report reads none of them (T17).

### The ratchet (my run, read-only)

`node cli/ratchet.ts`, exit 1, 27 violations, none in the report (T6):

- worse, task keys (7): `request-log-size` actions 31 > 14, looks 93 > 24, detours 3 > 1,
  glance median 3 > 1; `release` actions 26 > 25, looks 62 > 59, `keyboard.tabs` 400 > 170.
- worse, page keys (16), all `reading_load.*`: words on 8 pages (environment-and-admission
  840 → 2869, reference-cli 3445 → 4953, landing 2263 → 2890, observe 933 → 1509,
  specification 1089 → 1759, status 2792 → 3598, capabilities 1042 → 1328, getting-started
  1284 → 1375), long paragraphs on 4 pages, longest paragraph on 4 pages (capabilities 82 → 184,
  reference-cli 322 → 408).
- improved-not-taken (4): observe and specification, long paragraphs and longest paragraph.
- The baseline is now on `main` (commit a6920953), so the main comparison ran; it found nothing.

### Metrics whose names don't hold for this round

- **Page keys measure another site, not a change.** R-02 measured 82faeb87 plus R-00 to R-02's
  sets; R-03 measures 8591d014 plus an uncommitted docs pass. Between them `main` made 57 commits to
  `website/docs`: the source of `environment-and-admission.md` went from 801 to 2815 words on
  `HEAD`, and the uncommitted pass takes it to 2707. So the 16 page violations are new content
  from `main`, not a result of this round. The report prints `base` beside them without saying so,
  and has no operator notes to say it.
- **The `request-log-size` keys 31 / 93 / 3 / 3 are the unreached charge** (T2), on every cell.
  Its `detours` 3 comes from an unreached play (T1). Its `keyboard.tabs` 400 was already the
  charge in R-02 after (390x844, UXF-020), so the key cannot show that the 1280x800 keyboard plays
  now fail too (R-02 after: reached in 6 actions, 186 and 189 Tabs).
- **The `release` `keyboard.tabs` 400 is the charge for 3 unreached keyboard plays.** The fourth
  (390x844 light) reached with 324 Tabs on the same path that failed in dark (T19). The report's
  task table prints "tabs 25" and "154" for two failed plays: those are the Tabs counted before the
  reader gave up, not what it pressed (T18).
- **The known-failing count of gate-5-keyboard (2) hides a changed cause.** The UXF-020 mark
  (`match` "no Tab reaches") covers `request-log-size` 390x844 keyboard. UXF-020 is a keyboard
  defect whose pointer play passed (R-02 after: 14 actions). In R-03 the pointer play at 390x844
  fails too (§ 2.1), so the keyboard play would fail without UXF-020. The substring mark cannot
  tell the two causes apart.
- **`base` and `last` disagree on four task keys without a record.** `install` `keyboard.tabs`
  base 85, last 21; `release` actions base 25, last 21; looks base 59, last 47. The baseline holds
  them under decision `R-02-release-0.10.0`, which no round dir records.
- **`keyboard.tabs` passes the ratchet inside its 30 % band on moves the report shows as
  progress.** `observe` 28 → 21 and `topology` 32 → 21 (last) are within the band (T5).
- **`reading_load.words` now takes the 1280x800 value.** It is higher than 390x844 on 11 of 13
  pages (reference-cli 4953 against 4909). R-02's key took the 390 value, which held the table
  guidance line; the key changed viewport, not only content.
- **Worst pages are still a tie-break** (T11): 11 pages tie at 1 failure at 1280x800 (all UXF-008,
  known), all 13 tie at 0 at 390x844, reference-cli wins on words.
- **Trend R-03 against R-02 after is not comparable on the sums.** Actions 57 → 79, looks 96 → 180:
  `request-log-size`'s charge adds 17 actions and 69 looks, `release` adds 5 and 15 (T7). Without
  the charged task the sums are 48 and 87.
- **Each cell ran once** (T5). The theme pair of `release` 390x844 keyboard shows one run is not
  enough to judge a keyboard gate.

### Did R-02's change set move its metrics? (not in the report, T16)

| R-02 change | target metric | R-02 as measured | R-02 after | R-03 | holds |
|---|---|---|---|---|---|
| C1 phone tables (UXF-009) | gate 4 and 5 of `observe`, `topology`, `request-log-size` at 390x844 | 12 red or marked | 0 | `observe`, `topology` reached in 2 actions; `request-log-size` unreached for another cause (§ 2.1) | yes for the tables |
| C2 search moves to the row (UXF-017) | `request-log-size` at 390x844 reached | no | 14 actions | not reached: the search for "request records" now opens the Observe concept page, which has no anchored row | no longer exercised |
| C3 paths wrap in table cells (UXF-019) | docs tables wider than their column at 1280x800 | 1 | 0 | 0 (gate-1 52/0/0) | yes |
| C4 kickers name their set (UXF-012) | bare-count kickers | 2 | 0 | not measured by a key; its asset is not in the report (T17) | unknown from the report |
| unslop pass (UXF-018) | Getting started shows the latest release | 0.6.0 | 0.9.0 | 0.10.0 against the changelog's 0.18.0: the asset is red | no |

## § 2 Findings the report misses

1. **`request-log-size` regressed from reached in every pointer play (R-02 after) to reached in
   none.** In R-02 after the reader landed on the CLI reference by "run --observe", its search
   for "request records" found a result only on that page (dropped as "on this page", T13), it
   pressed Escape and scrolled to the row. In R-03 the same search finds "…/requests with the last
   requests it answered … Concepts › Observe › A served run's metrics and request log": new text
   on the Observe page from `main`. The reader leaves the truth page for it, and the section
   names neither the size nor `--observe-requests` (`plays/request-log-size--*--pointer.json`). The
   site has the answer one search hit away from the wrong page. The tree's `observe.md`, edited
   after the build, adds exactly that sentence and link to that section; the run did not see it.
2. **`release` by keyboard fails in 3 of 4 plays, unmarked and without a finding.** Each play
   follows the pointer path and fails on its last step: "no Tab reaches link "Changelog" in 400
   presses". The search for "release" now opens Getting started at the release sentence (R-02
   after: at "Build", lower down), and the reader walks the page from there by Tab. In 390x844
   light the same path reached the link (324 Tabs in all); in dark it did not, and the failure
   snapshot has the focus on the navbar's colour mode switch
   (`.out/test-results/tasks-L3-release-390x844-dark-keyboard-gating/error-context.md`). Either a
   keyboard user needs over 400 Tabs to reach the changelog link, or the walk is not repeatable
   (T19); one run cannot tell which.
3. **The regression asset of the closed UXF-018 is red.** `e2e/findings.spec.ts::UXF-018`:
   Getting started shows `b10x-spindle 0.10.0`; `CHANGELOG.md`'s latest release is 0.18.0. The
   finding is `status: closed`. The tree's `getting-started.md`, edited after the build, changes
   the line to 0.18.0; the built site still shows 0.10.0.
4. **The measured site is not the tree, and not kept.** `website/build` was written at 22:03:36,
   the plays ran 22:04 to 22:05, and `observe.md` and `getting-started.md` changed at 22:11. The
   built Observe page has no `observe-requests` (0 occurrences); the source has it. No frozen copy
   exists under `.out/`. The report says "commit 8591d014+uncommitted", which names neither state
   (T10).
5. **`install` at 1280x800 takes 2 actions and 86 Tabs (R-02 after: 1 action, 11 and 21).** The
   build command is no longer on Getting started's first screen at 1280x800; the reader needs the
   "Build" link of the page contents. The key stays 5 (the 390 value) and the Tabs sit inside the
   band of a baseline already raised to 85, so nothing flags it.
6. **No operator notes.** `R-03/README.md` is missing: no base, no cards rotated in, no triage
   rule, no list of the uncommitted edits in the measured build. R-02's triage rotated no new card
   for R-03, and the report does not say whether one was due.
7. **No list of open findings.** Six findings are open (UXF-006, 008, 010, 014, 015, 020);
   UXF-014 (Observe page does not name the topology endpoint) and UXF-015 lie on pages the
   uncommitted pass edits. The report names only UXF-008 and UXF-020, in the gate table.

## § 3 Tool defects

Open from R-02, checked against the R-03 source and run:

- **T1 holds.** `core/metrics.ts` `charged()` still keeps `detours` as counted. Shown by
  `plays/request-log-size--1280x800--light--pointer.json` (unreached, detours 3) under the key
  `request-log-size` `screen_only.detours` 3.
- **T2 holds.** `core/metrics.ts` `taskMetrics()`: one unreached cell sets the key to the charge.
  Shown by `plays/release--390x844--light--keyboard.json` (reached, 324 Tabs) under `release`
  `keyboard.tabs` 400, and by `request-log-size` `keyboard.tabs` 400 in both R-02 after and R-03
  while the 1280x800 keyboard plays went from reached to unreached.
- **T3 holds.** `core/bot.ts` `play()`, `reading = rank(pick, task.words) === 0 && …` (line 222).
  Shown by `plays/request-log-size--1280x800--light--pointer.json`: "run --observe" (third word)
  lands on the truth page; the reader does not read on, searches, and leaves the truth page.
- **T4 holds in the code, not shown in R-03.** `core/probe.ts` `candidates()` still gives every
  control its section heading as `context`, code-block buttons included; no R-03 play picks one
  (no "Copy code" or "Toggle word wrap" in `plays/`).
- **T5 holds.** `e2e/tasks.spec.ts`: each cell runs once. Shown by
  `plays/release--390x844--{light,dark}--keyboard.json`: same path, 324 Tabs reached against
  "no Tab reaches" at 154. The 30 % band (`BAND_KEYS`, `core/metrics.ts`) absorbs count noise; it
  cannot absorb a verdict that flips.
- **T6 holds.** `cli/round.ts` writes no ratchet result, no list of red verdicts and no finding
  column for unmarked failures. Shown by `report.md` ("fail 4", "fail 5", "9 of 220 verdicts
  were red") against the 9 unmarked verdicts and the 27 ratchet violations.
- **T7 holds.** `cli/round.ts` sums the trend row over every task, charges included. Shown by
  `trend.md` R-03 79 / 180 against R-02 after 57 / 96: 17 actions and 69 looks of it are the
  `request-log-size` charge.
- **T10 holds.** `cli/round.ts` writes "Site measured: website/build" and `8591d014+uncommitted`.
  It keeps no frozen copy and no hash of the build. Shown by
  `website/docs/concepts/observe.md` and `getting-started.md` (changed 22:11) against
  `website/build` (22:03:36); the built Observe page lacks the `--observe-requests` link the
  source has.
- **T11 holds.** `cli/round.ts`, worst pages and contact-sheet captions: tie-break by words, known
  failures counted as "gate failure(s)". Shown by `shots/worst--reference-cli--390x844--*.png`
  (0 failures, 13-way tie) and `shots/contact--1280x800--*.jpg` (11 pages with "1 gate
  failure(s)", all UXF-008).
- **T12 fixed.** `isSearchField` takes `combobox`. At 390x844 the keyboard reader searches in 8 of
  16 plays, the pointer reader in 8 of 16 (`plays/*--390x844--*`).
- **T13 holds in part.** `core/bot.ts` line 259 now writes "Escape (no result off this page)", so
  the path no longer calls a found result "no result". A result on the reader's page is still
  dropped (`onThisPage()`, `choose()`). No R-03 play presses Escape. R-02 after's
  `request-log-size` passes depended on the drop (§ 2.1).
- **T14 holds.** `cli/round.ts` per-task table prints a keyboard play's `reached` and `tabs` beside
  a pointer path it did not take. Shown by `report.md` row `release 390x844 dark` (pointer 26
  actions; keyboard 25 actions, not reached, tabs 154) and `request-log-size 390x844` (pointer 24;
  keyboard 22). No table has the keyboard path.
- **T15 holds in part.** The two wrong twists are corrected by hand in `scenarios/cards.json`
  (R02-phone-operator: "request records" before the flag; R01-caller: "one mention of topology").
  Nothing checks a task's words against its card's twists (no test names a twist). The `release`
  truth still accepts the changelog only, and its anchor in `scenarios/tasks.json` still reads
  "0.9.0 on 2026-11-10" (latest: 0.18.0).
- **T16 holds.** `cli/round.ts` writes no "did the last change set move its metrics" section.
  Shown by `report.md` against `R-02/triage.md`'s change set (the table in § 1 above).

New in R-03:

- **T17** The round leaves out every Playwright result other than L2 and L3. `Taskfile.yml`
  `ux:docs:round` runs `npx playwright test` with `ignore_error: true`, then `cli/round.ts write`,
  which reads only `.out/run/verdicts/`. The findings and break specs' results never reach the
  report, and the task exits 0. Shown by the measure log ("10 failed", UXF-018 among them, then
  "exit 0") against `report.md` ("9 of 220").
- **T18** `core/bot.ts` `step()` pushes the glance before `act()` and counts the action and its
  Tabs after it. When `act()` throws ("no Tab reaches"), the record has one glance more than
  actions and drops the Tabs of the failed walk. Shown by
  `plays/release--1280x800--dark--keyboard.json` (18 glances, 17 actions, `tabs` 25 after more
  than 400 presses) and the 4 other plays that stopped on "no Tab reaches".
- **T19** A keyboard walk is not repeatable on one state. `plays/release--390x844--light--keyboard.json`
  reaches "Changelog" in about 170 Tabs from the last reading scroll. `…--dark--keyboard.json`, on
  the same path, presses 400 and ends on the navbar's colour mode switch (`error-context.md`). `act()`
  marks its target by its index in a fresh `querySelectorAll` (`pick.index`), so a DOM change
  between `candidates()` and `act()` would mark another element. The cause is not isolated.
  Until it is, a gate-5-keyboard verdict from one run can flip by theme.

## § 4 Caveats to send with the round

- The run is red: 9 verdicts with no mark and no finding (`request-log-size` by pointer in all 4
  cells and by keyboard at 1280x800; `release` by keyboard in 3 of 4), and the UXF-018 regression
  asset, a closed finding, is red.
- The ratchet fails: 7 task keys and 16 page keys worse, 4 improvements not taken.
- The base moved to 8591d014: 57 commits of new docs content from `main`. The page keys measure
  that content, not this round.
- The measured build predates edits to `observe.md` and `getting-started.md` that address
  `request-log-size` and UXF-018. The build is not kept, so this round cannot be reproduced.
- `request-log-size` passed in R-02 after because the reader dropped an on-page search result; the
  new Observe text now takes the search away from the truth page.
- Keyboard results are single runs. One `release` cell reached and its theme twin did not, on the
  same path.
- No operator notes, no people, no questionnaire, no recordings. L4 and L5 not run.

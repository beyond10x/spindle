# R-01 report review

Reviewer 2 (report reviewer), from `references/review-agents.md` § 2. Inputs: `report.md`,
`round.json`, `shots/`, `README.md` (operator notes), `../R-00/` (`report.md`, `before/`, `after/`,
`triage.md`, `decision.md`, `review-report.md`), `../trend.md`, the raw run snapshot
(`.out/R-01-run/`: `pages/`, `measured/`, `plays/`, `verdicts/`, `measured.json`, `gates.json`),
`.out/playwright.json`, `baseline.json`, `known-failing.json`, `findings/`, the scenario cards and the
source of `core/`, `cli/`, `e2e/` and `adapter.ts`. Recomputed with `.out/review-2-R-01/recompute.py`
(standard library only, imports nothing from the project; output in `recompute.out`). Spot checks
against the frozen build `.out/R-01-build/` on port 4332 with `.out/review-2-R-01/spot.ts`
(`spot.json`). `npm test`: 44 pass. Nothing paid or live ran. Not available: recordings,
questionnaires (no people).

**Verdict: not fit to send as is.** One number differs, by 0.0005, from rounding; every other
number matches the raw run. But the report does not say that the run is red: 8 of 200 verdicts
fail with no mark and no finding (task `topology`, 0 of 8 plays reached). It also does not say that
the ratchet fails: 6 keys are worse than `baseline.json`, 1 improvement is noise, and 24 keys have no
baseline. One of the 6, `http` `keyboard.tabs` 25 → 35, is not the content change the README's
decision id covers. **Default action:** add § 4 to `report.md`. File a finding for `topology`, or mark
it against T3 if the triage judges it a reader defect. Do not take `observe` `screen_only.detours`
1 into the baseline. Take the 5 reading-load keys under `R-01-base-82faeb87` and nothing else under
that id. Leave `http` `keyboard.tabs` at 25 until T5 is fixed: on repeats it gives 25 or 35. Then
send.

## § 1 Are the numbers right and meaningful?

### Recomputed from the raw run

| number | report | recomputed | diff |
|---|---|---|---|
| page checks / measured-only / plays / verdicts | 48 / 72 / 56 / 200 | 48 / 72 / 56 / 200 | 0 |
| red as run | 8 of 200 | 8 (4 gate-4, 4 gate-5-keyboard, all `topology`) | 0 |
| gate-1 / gate-2 / gate-5 | 48/0/0, 0/0/48, 48/0/0 | same | 0 |
| gate-4 / gate-5-keyboard | 22/4/2, 22/4/2 | same; no stale mark | 0 |
| task keys, 7 tasks × 6 keys | table | identical | 0 |
| per-play cells (reached, actions, looks, detours, keyboard reached, tabs), 28 rows | table | identical | 0 |
| page keys, 12 pages × 8 keys | table | identical except one | see below |
| per-page cells (48) and measured-only cells (36) | table | identical | 0 |
| reading load: median words, long paragraphs | 1006, 26 | 1006, 26 | 0 |
| trend: actions, looks, axe, long, tasks, glance max | 126, 331, 0, 26, 7, 3 | same | 0 |
| `page.getting-started` `layout_shift.score` (report, `round.json`, trend) | 0.001 | 0.0005 | +0.0005 |

**One number differs.** The raw checks hold 0.0005, at 390x844 only. `checkPage` rounds the shift
to 4 places, and `pageMetrics` rounds it again to 3 places, half up, giving 0.001 (T8). It is well
under the 0.01 tolerance and changes nothing.

### Metrics whose names don't hold for this round

- **`topology`: 31 / 93 / 400 is the charge for "not reached", not a measurement.** All 8 plays
  failed. The plays themselves took 10 to 14 actions and 120 to 186 tabs. The values equal
  `observe`'s for the same reason. Read them as "failed", not as a cost to beat.
- **`observe` hides a regression at 1280.** It was reached at 1280 with 7 actions and 19 looks.
  R-00 after needed 6 actions and 16 looks: one more reading screen, because wave 18 lengthened the
  `run` section. The key shows 31, the cap charged for the unreached 390 plays, so the 1280 change
  cannot show (T2).
- **`observe` `screen_only.detours` 2 → 1 is not progress.** The 390 plays are still unreached. The
  cap now cuts their path one step before the off-site link `docs/design/observe.md`, which was
  the second detour in R-00. Unreached plays are not charged for detours (T1). The ratchet will call
  this `improved-not-taken`. Taking it would lower the bar through noise.
- **`http` `keyboard.tabs` 35 (base 25) is a coin toss, not a regression.** Only
  `1280x800 light keyboard` turned off to "The rally example →" (3 actions, 2 detours, 35 tabs). The
  dark keyboard play and both pointer plays took the R-00 path (2 actions, 25 tabs). I repeated the
  play 3 times per theme on the frozen build: 3 of the 6 runs took the detour (35 tabs), 1 light and
  2 dark. The other 3 did not (25). The report shows neither the regression nor the cause. The
  README's content decision does not cover it.
- **`install` at 1280 flips between 1 and 2 actions.** In the run, the dark keyboard play needed a
  scroll and the light one did not. Repeated: 2 of 6 keyboard runs and 1 of 6 pointer runs needed the
  scroll, in both themes. The truth's code block sits on the fold (R-00 T2, still open), so timing
  decides. The worst-of key hides this as long as 390 costs 5.
- **`http-call` at 390, 24 actions, includes 2 clicks on code-block buttons** ("Toggle word wrap",
  "Copy code to clipboard"). They are picked because their section heading carries "HTTP" (T4). It
  also comes within 6 actions of the 30-action cap, like `release` at 26.
- **Trend row R-01 is not comparable with R-00 after.** Actions 71 → 126 and looks 185 → 331 come
  from summing 7 tasks instead of 5. 62 of the 126 actions are cap charges. On the 5 tasks R-00 had,
  the sums are 71 and 185, unchanged (T7).
- **`always_visible.misses` counts an absent element twice.** It counts top and end separately, so
  the one missing search field makes 4 of the landing page's 4 and 4 of every docs page's 5. In
  gate 2, 96 of 118 failures are "search absent". Gate 2 still passes 0 of 48: it is fully masked
  until the search decision lands, and `R-01/decision.md` does not exist yet.
- **`reading_load.*` moves with content, as the README says.** The changes are environment and
  admission (+307 words, long paragraphs 0 → 2, max paragraph 68 → 121) and reference-cli (+191 words,
  long paragraphs 11 → 12). Words still differ by viewport on 6 pages (for example reference-cli
  3458 at 1280, 3549 at 390). The key takes the 390 value.
- **`layout_shift.score`** has no baseline value on any page, so the ratchet reports it as missing on
  all 12. The page table has no `last` column, though the README says `last` now reads `R-00/after/`.
  It does, for task keys only.
- **Worst pages are a tie-break again** (R-00 T10, open). At 1280, 11 of 12 pages tie at 3 failures,
  and at 390 all 12 tie at 2. Every one of those failures is known-failing. reference-cli wins on
  word count. The contact sheets caption every page "2 gate failure(s)" or "3 gate failure(s)", while
  the gate table shows gates 1, 2 and 5 with 0 fail.
- **Each cell ran once.** Repeats show that a cell does not repeat: `http` and `install` above.
  The run's own theme pairs show the same: `release` and `topology` tabs differ by 1, and
  `topology` at 390 stops after 12 actions by pointer and 14 by keyboard on the same path. Light and
  dark differ in no page key.

### The ratchet, which the report does not run

Against `baseline.json` as it stands (my own check, same rules as `core/ratchet.ts`):

- worse (6): `http` `keyboard.tabs` 35 > 25; environment and admission `words` 818 > 511,
  `long_paragraphs` 2 > 0, `max_paragraph` 121 > 68; reference-cli `words` 3549 > 3358,
  `long_paragraphs` 12 > 11;
- improved-not-taken (1): `observe` `screen_only.detours` 1 < 2;
- missing (24): 12 `layout_shift.score` keys, and the 6 keys each of `http-call` and `topology`.

`baseline.json` holds no `R-01-base-82faeb87` decision yet.

## § 2 Findings the report misses

1. **`topology` fails gate 4 and the keyboard gate in all 8 plays, with no finding and no mark.**
   The report shows it only as rows of the task table. At 1280 the reader clicks "run --observe" and
   lands on the CLI reference's `run` section. The answer is 4575 px further down, about 7 reading
   screens (spot check, `run` heading at scroll 3365, answer text at y 7940). It then leaves for
   "Observe", because it reads on only after a link that carried the task's first word, "topology",
   and no control carries it (T3). The body text "GET /__spindle/topology" on that page is never
   read (R-00 T8). At 390 the answer is in the same `--observe` table cell as `observe`'s. The cell
   starts at x 383, and its table box ends at x 374 (358 px wide, 746 px of scroll width), so it is
   off screen to the right (UXF-009). `topology` at 390 cannot pass even with a better reader.
2. **`topology` and `observe` test the same spot.** Both truths are in the one `--observe` cell of the
   CLI reference (157 words, 580 px tall; spot check). Any text of the cell in view counts as the
   truth in view (R-00 T2), so the R01-caller task adds no new truth position, only a new start
   page and words. A reader that read on after "run --observe" would score it reached on the same
   screen as `observe`, 333 px above the topology sentence.
3. **The R01-caller card's twist is wrong.** It says the Observe concept page "does not carry the
   word topology". It does, in "a Vue 3 app with a topology, an instance and a view screen" (section
   "The observe API and UI", which the reader visits). That is the UI's topology screen, a likely
   wrong scent for a person.
4. **Concept page kickers say "of 8" with 6 concept pages.** The R-00 change set added two sidebar
   links under Concepts (C3, C4), and the kicker counts them: "Concepts · 7 of 8" on Observe, "6 of 8"
   on Deployment targets. It shows on every concept frame of all four contact sheets. This is a
   regression from R-00's own change set that no metric sees.
5. **`http` by keyboard at 1280 detours through the rally example in about half of the runs**
   (2 detours, 35 tabs; 3 of 6 repeats). R-00 after showed the short path in all 4 plays. The run
   gives no sign of it, because each cell runs once (T5).
6. **`observe` at 1280 got one screen longer (6 → 7 actions, 16 → 19 looks)**, from wave 18's
   content in the `run` section. The key cannot show it (T2).
7. **The measured site is gone from `website/build`.** `website/build` was rebuilt at 16:23, two
   minutes after the run (`index.html` 45907 bytes against 45239 in `.out/R-01-build/`, different
   CSS and JS hashes). The report says "Site measured: website/build". Only the frozen copy
   reproduces this round.
8. **UXF-007 is still open on the anchor card A-newcomer.** Getting started still says "Spindle has
   no release yet" on the 390 first screen (contact sheet), while the `release` task's truth is a
   changelog with 0.9.0. The report does not list open findings at all.

## § 3 Tool defects

- **T1** `core/metrics.ts` `charged()`: an unreached play is charged for actions, glances and tabs,
  but its `detours` stay as counted. So a failing play that gives up earlier lowers
  `screen_only.detours`. Shown by `.out/R-01-run/plays/observe--390x844--*--pointer.json` (detours 1)
  against `R-00/after/round.json` (2, path ending in `docs/design/observe.md`).
- **T2** `core/metrics.ts` `taskMetrics()`: the worst over viewports and themes, with the cap at
  31 / 93 / 400, makes every key of a task with one unreached cell constant. Movement at the
  viewports that reach the truth cannot show. Shown by `plays/observe--1280x800--*--pointer.json`
  (7 actions, R-00 after 6) under `measured.json` `observe` 31.
- **T3** `core/bot.ts` `play()`, `reading = rank(pick, task.words) === 0 && …`: the reader reads on
  only after a link that carried the first task word. A link that carried the second word
  ("run --observe" for "--observe") is left at once for the next match. Shown by
  `plays/topology--1280x800--*--pointer.json`: path "run --observe" → "Observe", with the truth
  about 7 reading screens below the landing point.
- **T4** `core/probe.ts` `candidates()` with `core/bot.ts` `rank()`: a control matches on its
  section heading, so code-block utility buttons (word wrap, copy) under a heading that carries a
  task word are chosen and clicked as navigation. Shown by `plays/http-call--390x844--*--pointer.json`,
  steps 16 and 17.
- **T5** `e2e/tasks.spec.ts` with `core/bot.ts`: each cell runs once, and nothing checks that a play
  repeats. On one build the paths differ by theme: `http--1280x800--light--keyboard.json` (3 actions,
  35 tabs) against `--dark--keyboard.json` (2, 25); `install--1280x800--dark--keyboard.json`
  (2 actions) against light (1). With tolerance 0 on `keyboard.tabs`, a one-off path fails the
  ratchet. Spot check (`.out/review-2-R-01/spot.json`, 3 repeats per cell on the frozen build):
  `http` keyboard took the detour in 3 of 6 runs, and `install` needed the extra scroll in 2 of 6
  keyboard and 1 of 6 pointer runs, in both themes.
- **T6** `cli/round.ts`: the report neither runs nor summarises the ratchet. "Metrics against the
  bars" has no verdict column, and the page table has no `last` column. The red verdicts of the run
  are a count in a sentence ("8 of 200"), with no list and no finding column for unmarked failures.
  Shown by `report.md` against the 31 ratchet violations above.
- **T7** `cli/round.ts` trend row: the sums run over whatever tasks the round has, cap charges
  included. Rows written before a column existed get "–", even where the value is known: R-00 had 5
  tasks. Rows are kept in write order (R-00 after, R-00 before, R-00), and the R-00 rows were measured
  with different tool versions (65 against 99 actions), which no column says. Shown by `trend.md`.
- **T8** `core/page.ts` `checkPage()` and `core/metrics.ts` `pageMetrics()`: the layout shift is
  rounded twice (4 places, then 3 places half up). Shown by
  `pages/getting-started--390x844--light.json` (0.0005) against `measured.json` (0.001).
- **T9** `cli/round.ts` `levels`: L6 is hard-coded "no people in R-00" and appears in R-01's
  `round.json`. L7 states that the two review agents ran when the round is written, before they
  have.
- **T10** `cli/round.ts` "Site measured" and `commit`: they name a path and a `git rev-parse` with
  `+uncommitted`, not a hash of the built site. R-00 T6's build half is not fixed. Shown by
  `website/build/index.html` rebuilt at 16:23 and differing from `.out/R-01-build/index.html`.
- **T11** `cli/round.ts` worst pages and contact-sheet captions: still the R-00 T10 tie-break. Known
  failures count as "gate failure(s)". Shown by `shots/contact--390x844--*.jpg`, where all 12 pages
  read "2 gate failure(s)".
- Still open from R-00 and active in this round: R-00 T2 (any line of a block is the truth in view:
  `install` at 1280, the shared `--observe` cell), R-00 T3 (no sideways scroll: `observe` and `topology` at
  390), R-00 T8 (controls only, prefix match), R-00 T9 (keyboard scrolling is scripted).

## § 4 Caveats to send with the round

- The run is red: `topology` reached 0 of 8, with no mark and no finding yet. Its 31 / 93 / 400 is a
  charge for failing, and so is `observe`'s.
- The ratchet fails as measured: 6 worse, 1 improved-not-taken, 24 missing. The 5 reading-load keys
  are the wave-18 content of the README's decision id. `http` `keyboard.tabs` 35 is a one-play
  path change, not content. `observe` detours 1 is noise; do not take it.
- The trend row's jump (71 → 126 actions) comes from 2 new tasks. On the 5 old tasks nothing moved,
  except `observe` at 1280, one screen worse, which the key hides.
- `topology` repeats `observe`'s truth cell. At 390 it fails for UXF-009's reason.
- Each cell ran once, and cells do not repeat. On the frozen build, `http` keyboard gives 25 or 35
  tabs and `install` at 1280 gives 1 or 2 actions.
- The measured build is `.out/R-01-build/`, not `website/build` (rebuilt after the run).
- Concept pages say "n of 8" for 6 pages, a side effect of R-00's sidebar links.
- No people, no questionnaire, no recordings. Gate 2 is fully masked until the search decision
  (`decision.md`) is written and built. L4 and L5 were not run.

# R-00 report review

Reviewer 2 (report reviewer), from `references/review-agents.md` § 2. Inputs: `report.md`,
`round.json`, `shots/`, `../trend.md`, the raw run (`.out/run/pages`, `measured`, `plays`,
`verdicts`), the tool source, `known-failing.json`, `baseline.json`, `findings/`. Spot checks ran
against the served R-00 build (`build/index.html` is from 10:06 and unchanged since the run) with
Playwright scripts in `.out/review-2/`. `npm test`: 41 pass. Not available: previous round,
questionnaire, operator notes, recordings, round README.

**Verdict: not fit to send as is.** Every number in the report matches the raw run. Some of them
don't mean what their names say: two task metrics count the steps before a failed task gave up, one
anchor value comes from a hover side effect, one task cannot pass at 390 px whatever the
navigation, and the run was red until the known-failing marks were written after it.
**Default action:** add § 4 to `report.md` and send. Fix T1, T3 and T4 before R-01, and take the
`install`, `http` and `observe` baseline values again after those fixes.

## § 1 Are the numbers right and meaningful?

### Recomputed from the raw run

I recomputed them with `.out/review-2/recompute.mjs`, which imports nothing from the project.

| number | report | recomputed | diff |
|---|---|---|---|
| page checks / measured-only / plays / verdicts | 48 / 72 / 40 / 184 | 48 / 72 / 40 / 184 | 0 |
| gate-1 pass/fail/known | 48/0/0 | 48/0/0 | 0 |
| gate-2 | 0/0/48 | 0/0/48 | 0 |
| gate-4 | 12/0/8 | 12/0/8 | 0 |
| gate-5 | 20/0/28 | 20/0/28 | 0 |
| gate-5-keyboard | 12/0/8 | 12/0/8 | 0 |
| task keys (actions, looks, detours, glance median/p95, tabs), 5 tasks | table | identical | 0 |
| page keys (axe per theme, overflow, misses, words, long, max), 12 pages | table | identical | 0 |
| per-page and measured-only cells | table | identical | 0 |
| trend: actions 65, looks 153, axe 46, long paragraphs 23, median words 1006 | | identical | 0 |

**No number differs.** The `base` column equals the measured value everywhere, because
`baseline.json` was written from this run (10:29:30, same second as `.out/measured.json`).
`last` is "–" everywhere. Nothing in R-00 is compared with anything else.

### Run state the report does not show

- **The run was red.** The stored verdicts (written 10:23) hold `ok: false, finding: null` on 92
  of 184 verdicts: all 48 gate-2, 28 gate-5, 8 gate-4 and 8 gate-5-keyboard. `known-failing.json`
  and the findings were written 10:28. `cli/round.ts` judges the stored verdicts again against
  today's marks, so the report shows "0 fail". That is legitimate for a bootstrap round, but the
  report should say that the marks were fitted to this run.
- **Gate 2 checks nothing new.** It has 0 passes out of 48. Every check is masked by the
  "search", "theme toggle" and "docs sidebar" marks.

### Metrics whose names don't hold

- `screen_only.actions`, `screen_only.looks`, `screen_only.detours` and `keyboard.tabs` for `http`
  (19, 48, 2, 98) and `observe` (12, 20, 3, 95) come from plays that never reached the truth (0 of
  8 each). They count the steps until the reader gave up, not the cost of finding the answer. As
  lower-is-better ratchet values, they would score a reader that gives up sooner, or one that
  crashes, as an improvement (T4).
- **`install` actions at 390, 2.** This is a baseline value on an anchor card, and it comes from
  a hover side effect. The pointer path clicks `link "Direct link to Build"`, the heading's `#`
  anchor. That anchor has opacity 0 unless hovered. It shows only because the mouse is left over
  the "Build" heading after clicking "Get started →" (checked: opacity 0 with the mouse away,
  1 after the click). A phone has no hover. The keyboard play at the same size needs 5 actions
  (T1).
- **`install` at 1280, 1 action.** This rests on the truth's code line ending about 5 px above
  the fold (line top 771, viewport 800). The in-view test accepts any line of that code block,
  so the result is fragile (T2).
- **`looks`** is the sum of the glance levels. A scroll counts 3. It is not a count of looks.
- **`glance_cost.median` and `p95`** are 3 on 4 of the 5 tasks, the highest level there is. They
  separate nothing.
- **`always_visible.misses`** sums element × state pairs and takes the worst over the viewports.
  The result is 4 on all 12 pages, always the 390 px value, so the key carries no per-page
  information. UXF-008 names this key as its metric, but its 1280 px sidebar miss can never move
  it. Fixing UXF-002 moves it from 4 to 3 on docs pages, not to 2 (T7).
- **`axe nodes (sum)` = 46 in `trend.md`** counts the same 23 `link-in-text-block` nodes twice,
  once per theme. Light and dark give identical numbers on every page check and every play.
- **`reading_load.*`** on the landing page measures all of `main`, because there is no
  `article .markdown`: 2249 words, including the status list. `reference-cli` (3358 words,
  11 long paragraphs) and `status` (2791) are generated pages. A new CLI flag or status row fails
  the ratchet with no UI change. Word counts also differ between the two viewports (for example
  `docs` 446/459) because of text that shows at only one width.
- **The "worst page" is a tie-break.** At 1280, 7 pages tie at 4 counted failures, and at 390 they
  tie at 5. `getting-started` wins on word count. The count includes known-failing items and
  counts axe rules, not nodes: `docs` has 8 nodes against 7. The contact-sheet captions say
  "n gate failure(s)" while the gate table says 0 fail.
- **Gates 1 and 2 read two states**, page top and page end, with every menu closed. The skill
  asks for every state change. The open drawer and the open "On this page" panel are never
  checked.
- **The cards are played only as a start page and a word list.** None of the planned twists is
  played. There were no people, no questionnaire and no recordings. Each cell was run once, so
  stability is not shown.

## § 2 Findings the report misses

1. **`observe` cannot pass at 390 px, even after UXF-005's planned sidebar link.** The truth
   ("Record every request the listener answers outside", the `--observe` row of the CLI
   reference) sits in a table cell at x 383 to 762. The cell is inside `.b10x-table-wrap`, which
   is 358 px wide with a scroll width of 746. With the row scrolled into view, the truth's
   on-screen glance is still 3 (not in view). A phone reader must swipe the table to read it, and
   the reader bot never scrolls sideways (T3).
2. **Pointer and keyboard disagree on `install` at 390: 2 actions against 5.** The report shows
   only the pointer path. The keyboard path is "Get started →", menu, close, "On this page",
   "Build".
3. **UXF-004's evidence overstates.** The landing page carries "HTTP" in its text ("HTTP ingress
   … run serves the accepted commands and views over HTTP", below the fold), and so does the
   docs overview ("HTTP ingress"). Neither is a link. The finding's point holds: no control
   carries the word. But "nothing on screen carries HTTP" is a statement about controls only
   (T8).
4. **The first `http` click is driven by "runs".** "Get started →" is chosen because the task
   word "run" prefix-matches "runs" in the landing hero heading ("Deploy the specification.
   Spindle runs it."). Every hero link inherits that match.
5. **UXF-006 (S3) and UXF-007 (S2) are absent from the report.** UXF-007 is the content conflict
   on the A-newcomer anchor card: Getting started says "Spindle has no release yet", while the
   changelog lists 0.9.0. The 390 contact sheet shows the sentence on the first screen.
6. **On a phone, every docs page opens with a header card and a monospace list of source files.**
   On concept pages the card takes about half the first screen (contact sheet 390, both themes).
   No metric counts first-screen cost. This is one reason `install` at 390 has the truth below the
   fold.
7. **The 390 `release` play takes 26 of the bot's 30-action cap.** If the status page grows by
   about 4 screens, it becomes an unmarked gate-4 failure. A "Changelog" link is in the footer of
   every page, including the landing page.

## § 3 Tool defects

- **T1** `core/bot.ts` `act()`, and `playwright.config.ts`: the pointer click leaves the mouse at
  the click point, so the next page's `:hover` state reveals controls that are otherwise hidden.
  The 390 viewport emulates neither touch nor a mobile device. Shown by
  `.out/run/plays/install--390x844--*--pointer.json` (path `link "Direct link to Build"`) against
  the `--keyboard` plays (5 actions). `.out/review-2/check.ts` shows opacity 0 with no hover and
  1 after the click.
- **T2** `core/probe.ts` `textGlance()`: a match is judged per holder block (`pre`, `tr`, `p`).
  Any text node of the block in view counts as the truth in view. Clipping by a scroll box and
  opacity are ignored. Shown by `getting-started` at 1280: the code block runs from y 684 to 833,
  and its "git clone" line alone would satisfy the check. Also the CLI `#run` cell at 1280
  (x 683 to 1062) inside a wrap 703 px wide (`.out/review-2/check2.ts`, `check3.ts`).
- **T3** `core/bot.ts`: the bot never scrolls a box sideways. A truth inside a wide table at
  390 px can never be in view. Shown by `.out/run/plays/observe--390x844--*` and the cell at
  x 383 in a box 358 px wide.
- **T4** `core/metrics.ts` `taskMetrics()`, with `core/ratchet.ts`: `reached` is ignored, so
  failed plays feed lower-is-better keys. `known-failing.json` marks gate-4 and gate-5-keyboard
  for `http` and `observe` without `match`. Those marks also cover a thrown error ("no Tab
  reaches …") and the 30-action cap. A crash then shows as `improved-not-taken`, and
  `--update` would write it into the baseline. Shown by `baseline.json` `http` and `observe`
  values from 0-of-8 plays.
- **T5** `core/gates.ts` `covers()`: a mark matches any failure containing a substring, whatever
  the state or reason. The UXF-008 mark ("docs sidebar") was written for the end-state miss
  "outside the viewport". It would also hide a top-state "docs sidebar absent or not shown".
  Shown by `known-failing.json` line 8.
- **T6** `cli/round.ts`: it judges stored verdicts again against today's marks without reporting
  that the run itself failed (92 verdicts stored `ok: false`). Shown by
  `.out/run/verdicts/L2--docs--1280x800--light.json`. It also records `commit` from
  `git status -- ..`, which includes the untracked `website/ux/` tooling, so the label reads
  `+uncommitted` whatever the site state. Nothing records which build was measured: `../build` is
  never checked against the source. `src/css/custom.css` and `sidebars.ts` changed at 10:31 and
  10:32, after the round, and the label cannot tell those changes apart.
- **T7** `core/metrics.ts` `pageMetrics()` and the trend row in `cli/round.ts`:
  `always_visible.misses` is worst over viewports, so it is 4 on every page and hides the
  1280-only UXF-008. The trend's `axe nodes (sum)` adds both themes, so 23 nodes show as 46.
  `reading()` in `core/probe.ts` falls back to `main` on the landing page (2249 words). Shown by
  the `measured` page keys in `round.json`.
- **T8** `core/bot.ts` and `core/probe.ts` `candidates()`: only controls are matched, on their
  name or the nearest preceding H1 to H4. Body text is never read, yet the stuck message says
  "nothing on screen carries …". Words match as prefixes ("run" matches "runs"). Shown by the
  `http` plays and the landing hero context "Deploy the specification. Spindle runs it.".
- **T9** `core/bot.ts` keyboard mode: scrolling is `window.scrollBy`, not a key press. Tabs are
  counted from wherever focus last was. So "keyboard alone" is not proven for reading down a
  page. Shown by `http--1280x800--*--keyboard.json`: 17 actions, 13 tabs, 15 of them scrolls.
- **T10** `cli/round.ts` worst pages and contact sheets: the worst page is chosen by tie-break.
  `failuresOf` counts axe rules, not nodes, and includes known-failing items. The caption says
  "gate failure(s)". The shot is of the page top, so it shows none of the end-state misses.
  Shown by `shots/contact--*.jpg`, where 7 pages tie.
- **T11** `playwright.config.ts` and `e2e/setup.ts`: the JSON reporter and `outputDir` stay in
  `.out/` whatever `UX_OUT` says. Any run without `UX_OUT` deletes `.out/run`, and the round dir
  keeps no copy of the raw page checks and plays. Shown by `.out/playwright.json`, which a
  3-test run at 10:32 overwrote after the round (2 unexpected failures).

## § 4 Caveats to send with the round

- R-00 is its own baseline. `base` equals the measured value and `last` is empty, so no movement
  can be read from it.
- The known-failing marks were written after the run, to cover what failed. Gate 2 is fully
  masked (0 of 48 pass).
- `http` and `observe` were never reached. Their actions, looks, detours and tabs are steps to
  giving up, not costs, so do not read a drop as progress.
- `install` at 390 (2 actions) comes from a hover side effect. The keyboard play needs 5.
- `observe` at 390 cannot pass by a navigation change alone: the answer sits in a table that
  scrolls sideways.
- Light and dark give identical results. Each cell ran once.
- Reading-load numbers include generated pages and the whole landing page.
- No people, no questionnaire, no recordings. Twists are not played. L4 and L5 were not run.
- Open content conflict not in the report: Getting started says there is no release, while the
  changelog lists 0.9.0 (UXF-007, S2).

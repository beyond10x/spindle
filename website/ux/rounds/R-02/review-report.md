# R-02 report review

Reviewer 2 (report reviewer), from `references/review-agents.md` § 2. Inputs: `report.md`,
`round.json`, `shots/`, `README.md` (operator notes), `../R-01/` (`report.md`, `after/report.md`,
`after/round.json`, `triage.md`, `decision.md`, `review-report.md`), `../trend.md`, the raw run
snapshot (`.out/R-02-run/`: `pages/`, `measured/`, `plays/`, `verdicts/`, `measured.json`,
`gates.json`), `baseline.json`, `known-failing.json`, `findings/`, `scenarios/`, and the source of
`core/`, `cli/`, `adapter.ts` and `harness/server.ts`. Recomputed with
`.out/review-2-R-02/recompute.py` (standard library only, imports nothing from the project; output
in `recompute.out`). Ratchet: `node cli/ratchet.ts --measured .out/R-02-run/measured.json`
(`ratchet.out`). Spot checks against the frozen build `.out/R-02-build/` on port 4332 with
`.out/review-2-R-02/spot.mjs` (Playwright only; `spot.json`). Nothing paid or live ran. Not
available: recordings, questionnaires (no people).

**Verdict: not fit to send as is.** No number differs: every count, gate, cell, key, sum and
worst page recomputes to the report's value. But the report does not say that the run is red:
4 of 220 verdicts fail with no mark and no finding (task `request-log-size` at 390x844, pointer and
keyboard, both themes). It does not say that the ratchet fails (8 violations: 6 missing, 2
improvements that are noise). It does not answer the README's triage rule 4 (did R-01's five
changes move their metrics). And every keyboard play at 390x844 skips the search field the pointer
play uses, so `keyboard.tabs` and gate-5-keyboard at 390 measure another journey. **Default
action:** add § 4 to `report.md`; mark `request-log-size` at 390x844 against UXF-009 (gate-4 and
gate-5-keyboard, `match` "not reached"), as the README already names the cause, and add the task to
UXF-009's sessions; take the 6 `request-log-size` keys in as new keys; do not take `install` and
`http` `keyboard.tabs` (18, 27) into the baseline; fix T12 before the keyboard keys of any task
that searches are read as progress. Then send.

## § 1 Are the numbers right and meaningful?

### Recomputed from the raw run

| number | report | recomputed | diff |
|---|---|---|---|
| page checks / measured-only / plays / verdicts | 52 / 78 / 64 / 220 | 52 / 78 / 64 / 220 | 0 |
| red as run | 4 of 220 | 4 (2 gate-4, 2 gate-5-keyboard, all `request-log-size` 390x844) | 0 |
| gate-1 / gate-2 / gate-5 | 52/0/0, 30/0/22, 52/0/0 | same; the 22 known are all "docs sidebar" at 1280x800 (UXF-008) | 0 |
| gate-4 / gate-5-keyboard | 26/2/4, 26/2/4 | same; no stale mark | 0 |
| task keys, 8 tasks × 6 keys (report table, `round.json`, `measured.json`) | table | identical | 0 |
| per-play cells (reached, actions, looks, tabs, detours), 64 plays | table | identical | 0 |
| page keys, 13 pages × 8 keys, and their `base` | table | identical | 0 |
| task `base` and `last` (`R-01/after/round.json`) | table | identical | 0 |
| per-page cells (52) and measured-only cells (39) | table | identical | 0 |
| reading load: median words, long paragraphs | 987, 26 | 987, 26 | 0 |
| trend row: actions, looks, axe, long, tasks, glance max, shift max | 132, 347, 0, 26, 8, 3, 0.005 | same | 0 |
| worst pages and their failure counts | reference-cli, 1 / 1 / 0 / 0 | same | 0 |

**No number differs.** R-01's T8 (double rounding) is fixed: `status` holds 0.004949 at
1280x800 light and the key is 0.005, rounded once.

### Metrics whose names don't hold for this round

- **`request-log-size` 31 / 93 / 1 / 3 / 3 / 400 is the charge for "not reached", not a cost.**
  The 4 plays at 390x844 failed after 26 actions (pointer) and 17 or 18 (keyboard, 167 and 170
  tabs). At 1280x800 the task took 3 actions, 3 looks, 124 or 128 tabs; the key cannot show it
  (T2). Its `detours` 1 is counted only as far as the failing plays got (T1).
- **`observe` and `topology` 31 / 93 / 400 are the same charge**, as in R-01. At 1280x800 both now
  take 1 action (R-01 before: 7, and unreached), the effect of R-01's C2; the keys stay at 31
  because 390x844 is unreached (T2). The report's table shows them as "no movement".
- **`request-log-size` at 1280 is reached by the row below, not by the row it looks for.** The
  reader follows "run --observe" (it carries the third word, "observe") and lands on `#run--observe`;
  the `--observe-requests` row is the next one and its answer line sits at y 743 of 800 (spot), so
  the play counts as reached. It never reads on after that link (T3). The answer's position was found by luck of
  adjacency, not by the words the card gives the persona.
- **`keyboard.tabs` at 390x844 is not the pointer path by keyboard.** In 12 of 12 keyboard plays at
  390 whose pointer play searched (`environment`, `http-call`, `observe`, `release`,
  `request-log-size`, `topology`), the keyboard reader never uses the search field (T12). So
  `http-call` keyboard at 390 takes 24 actions and 72 tabs while its pointer play takes 5 actions;
  `environment` 6 against 5; `release` 26 against 21. The report's task table prints these tabs
  beside the pointer path, under "tabs from the keyboard play at the same viewport and theme". The
  keys `http-call` 72 and `release` 131 are no-search journeys.
- **`install` 18 (base 19) and `http` 27 (base 30) `keyboard.tabs` are noise.** The site is R-01
  after's (same commit, same change sets, every page key identical). Between R-01 after and R-02
  only keyboard tab counts moved, every pointer path is identical, and the moves go both ways on
  the same paths: `observe` 1280 26 → 21 and 30 → 25, `topology` 1280 26 → 29 and 26 → 28,
  `topology` 390 light 128 → 138, `http` 1280 light 30 → 27, `install` 390 dark 19 → 18. Within
  R-02, light and dark differ by 1 to 6 tabs on the same path (`observe` 1280 21/25, `topology`
  390 132/138, `request-log-size` 124/128 and 167/170, `release` 129/131). The ratchet calls the two
  task values `improved-not-taken`; taking them lowers the bar on noise (T5).
- **`page.status` `layout_shift.score` 0.005 is one cell.** Only 1280x800 light shows a shift
  (0.004949); dark and both 390 cells read 0, R-01 after read 0, and three repeat loads of the
  frozen build at 1280 light read 0 (spot). Under the 0.01 tolerance, so
  the ratchet passes it; the trend column "layout shift (max)" 0 → 0.005 is that one cell.
- **Trend R-02 against R-01 after is not comparable on the sums.** Actions 101 → 132 and looks
  254 → 347 are the new task's cap charge (31, 93). On the 7 R-01 tasks the sums are 101 and 254,
  unchanged (T7). 93 of the 132 actions are cap charges.
- **`reading_load.words` takes the 390 value, which includes UI text.** Words differ by viewport
  on 10 of 13 pages: +13 on every page with a table at 390 (the "More columns" guidance line
  inside the article), +91 on reference-cli (7 tables), −9 on search. The key moves when the table
  guidance does, not only with content.
- **`always_visible.misses` 1 per docs page is UXF-008 only.** The search field is no longer
  missing anywhere (R-01's C1); every remaining miss is "docs sidebar outside the viewport" at the
  page end, 1280x800, and is known-failing.
- **Worst pages are still a tie-break** (T11). At 1280x800, 11 of 13 pages tie at 1 failure (UXF-008,
  known); at 390x844 all 13 tie at 0, and reference-cli wins on word count. The 390 worst shot is a
  page with no failure.
- **The `release` truth no longer matches the site's own answer.** R-01's C4 made Getting started
  name "GitHub release" (link to the releases page) in its first screen. The task's truth is the
  changelog URL only, so a reader who follows the release sentence is not done. The plays never
  read that sentence: they search "release", land on `#build` ("cargo build --locked --release",
  below the sentence) and read 12 screens down to the "Changelog" link. 17 to 26 actions measure the
  truth's narrowness as much as the site.
- **Each cell ran once.** See T5; the run's own theme pairs show it.

### The ratchet (my run, read-only)

`node cli/ratchet.ts --measured .out/R-02-run/measured.json`, exit 1, 8 violations:

- improved-not-taken (2): `install` `keyboard.tabs` 18 < 19, `http` `keyboard.tabs` 27 < 30. Noise,
  above; do not take.
- missing (6): the 6 keys of `request-log-size`, a new task. Take in as new keys; they are cap
  charges, so they hold the bar at "unreached" and any reach at 390 will show.
- worse: none. `status` layout shift 0.005 is within 0.01.
- `website/ux/baseline.json` is not on `main` (the whole `ux/` tree is uncommitted), so the
  baseline is not compared with main and no `baseline-worse-without-decision` check runs.

### Did R-01's changes move their metrics? (README, triage rule 4; not in the report)

| R-01 change | target metric | R-01 before | R-02 | moved |
|---|---|---|---|---|
| C1 offline search (UXF-001) | gate-2 "search" misses; `http-call`, `environment` actions | 48 misses; 24, 6 | 0 misses; 5, 5 | yes, pointer only: by keyboard at 390 `http-call` is still 24 actions, `environment` 6 (T12) |
| C2 row anchors (UXF-013) | `topology` gate 4 at 1280; `observe` actions at 1280 | unreached; 7 | reached in 1 action; 1 | yes; the keys stay 31 (T2) |
| C3 sidebar shortcuts as HTML items (UXF-016) | concept kickers "n of 6" | "n of 8" | "1 of 6" to "6 of 6" (`shots/contact--390x844--light.jpg`) | yes |
| C4 release sentence (UXF-007) | sentence agrees with `CHANGELOG.md` | "no release yet" | "attached to each GitHub release", linked to the releases page, on the 390 first screen (y 578) | yes |
| C5 14 px key data (UXF-011) | text under 14 px in those elements | under 14 px | every table cell and code block of the CLI reference at 14 px (spot) | yes |

## § 2 Findings the report misses

1. **`request-log-size` fails gate 4 and gate-5-keyboard at 390x844 in all 4 plays, unmarked and
   without a finding.** The report shows "fail 2" twice and the rows; it never says which. The
   cause is UXF-009, as the README and the card's own twist predict: the `--observe-requests` row's meaning cell starts at x 431 and the text at x 446, in a table 836 px wide inside a 358 px box, on a 390 px screen (`spot.json`). The reader
   lands on the right page, scrolls it to its end (15 screens) and never has the answer cell in
   view. No reader change can pass it.
2. **The search does not answer the card's words on the truth page.** "request records" returns one result, "…refuses only the request that selects it…" under CLI reference › run: not the row, and the two words are not adjacent there. "--observe-requests" and "observe-requests" return 7 results, none of them the row (`spot.json`). The index stops at section headings, so at best it lands on the `run` heading, far above the row. On the phone the reader was already on the CLI reference, dropped the one result as "on this page" and wrote "Escape (no result)" (T13).
3. **By keyboard at 390x844 the search field is never used** (0 of 16 keyboard plays at 390, against
   12 of 16 pointer plays; at 1280 both modes search in the same 6 plays). The phone keyboard user of R-01's C1 is not measured.
   After a keyboard close of the menu the site moves focus into the search field and widens it (`probe-kb.json`), so a keyboard user is one key from searching; the reader scrolls instead (T12).
4. **`request-log-size` at 1280 needs 124 to 128 Tabs for 3 actions**, the highest keyboard cost at
   1280 (`release`: 94): the reader tabs through the sidebar and the in-page contents
   to the "run --observe" link. The key shows 400 (the 390 charge), so nobody sees it.
5. **The R02-phone-operator card contradicts its task.** The twist says the operator scans for
   "request records", "not the flag name"; the task's words give the reader the flag name
   `--observe-requests` as its second word. At 1280 the play succeeds through the third word,
   "observe", which neither twist names.
6. **The R01-caller card's twist is still wrong.** It says the Observe concept page "does not carry
   the word topology"; it does: "ui/ is a Vue 3 app with a topology, an instance and a view screen" (`spot.json`). R-01's review reported it; `scenarios/cards.json` and this
   round's `round.json` still carry it. At 390 the search for "topology" from that page leads to
   Status ("Vue 3 app in ui/ with topology"), 20 reading screens, and the cap.
7. **The measured site is gone from `website/build` again.** `website/build/index.html` was rebuilt
   at 17:00:47, after the snapshot at 16:59, with other CSS and JS hashes
   (`styles.7d445912.css` against `styles.ab8d364f.css` in `.out/R-02-build/`). The report says
   "Site measured: website/build". Only the frozen copy reproduces this round (T10).
8. **No open-findings list.** UXF-009 now blocks three tasks (`observe`, `topology`,
   `request-log-size`), all 12 unreached plays of the 32 at 390x844; the report names it
   only as a gate-table finding id.

## § 3 Tool defects

Open from R-01, checked against the R-02 source and run:

- **T1 holds.** `core/metrics.ts` `charged()` still leaves `detours` as counted. Shown by
  `plays/request-log-size--390x844--*--pointer.json` (detours 1, unreached) under `measured.json`
  `request-log-size` `screen_only.detours` 1.
- **T2 holds.** `core/metrics.ts` `taskMetrics()`: one unreached cell fixes every key at the cap.
  Shown by `plays/observe--1280x800--*--pointer.json` and `plays/topology--1280x800--*` (1 action,
  R-01 before 7 and unreached) and `plays/request-log-size--1280x800--*` (3 actions) under keys of
  31 / 93 / 400.
- **T3 holds.** `core/bot.ts` `play()`, `reading = rank(pick, task.words) === 0 && …`. Shown by
  `plays/request-log-size--390x844--light--pointer.json`: "run --observe" (third word) lands on the
  truth page and the reader turns to the menu and the search instead of reading.
- **T4 holds.** `core/probe.ts` `candidates()` with `core/bot.ts` `rank()`: code-block buttons
  matched by section heading. Shown by `plays/http-call--390x844--*--keyboard.json` ("Toggle word
  wrap", "Copy code to clipboard" after "Serving ingress.network over HTTP").
- **T5 holds.** `e2e/tasks.spec.ts` with `core/bot.ts`: each cell once, tab counts vary on one
  path. Shown by `plays/observe--1280x800--{dark,light}--keyboard.json` (21, 25, path "run
  --observe"), `plays/topology--390x844--{dark,light}--keyboard.json` (132, 138) and against
  `R-01/after/round.json` (`http` 1280 light keyboard 30 → 27, `install` 390 dark keyboard 19 →
  18, same paths); the ratchet asks to take two of them in.
- **T6 holds.** `cli/round.ts`: no ratchet result, no list of red verdicts, no finding column for
  unmarked failures. Shown by `report.md` ("fail 2", "4 of 220 verdicts were red") against the 4
  unmarked `request-log-size` verdicts and the 8 ratchet violations.
- **T7 holds.** `cli/round.ts` trend row sums over whatever tasks the round has, cap charges
  included. Shown by `trend.md` R-02 132 / 347 against R-01 after 101 / 254 with no task moved.
- **T8 fixed.** `core/page.ts` no longer rounds; `pageMetrics` rounds once. `pages/status--1280x800--light.json`
  0.004949 → 0.005.
- **T9 fixed** in L6 (`${round}`); L7 now says the reviews run after the report.
- **T10 holds.** `cli/round.ts` writes "Site measured: website/build" and a `+uncommitted` commit,
  not a hash of the built site. Shown by `website/build/index.html` (17:00:47) differing from
  `.out/R-02-build/index.html` (16:59).
- **T11 holds.** `cli/round.ts` worst pages and contact-sheet captions: tie-break by words; known
  failures counted as "gate failure(s)". Shown by `shots/worst--reference-cli--390x844--*.png`
  (0 failures, 13-way tie) and `shots/contact--1280x800--*.jpg` ("1 gate failure(s)" on 11 pages,
  all UXF-008).
- R-00's T2 (any line of a block counts), T3 (no sideways scroll: `request-log-size`, `observe`,
  `topology` at 390) and T8 (controls only) hold; R-00 T9 (scripted keyboard scrolling) holds.

New in R-02:

- **T12** `core/bot.ts` `play()` search branch with `act()` in keyboard mode: at 390x844 the keyboard
  reader never uses the search field, while the pointer reader does in the same state. Shown by
  `plays/*--390x844--*--keyboard.json`: 0 of 16 search, against 12 of 16 pointer plays
  (`environment--390x844--light--keyboard.json`: reveal → dismiss → scroll ×3, 6 actions;
  `--pointer.json`: reveal → dismiss → search, 5 actions). At 1280 the two modes agree. Spot probe (`probe-kb.json`): after a keyboard Enter on "Close navigation bar" the focus moves into the search field, which widens from 44 to 144 px; right after the key the closing backdrop (`.navbar-sidebar__backdrop`) still covers the field's centre, 300 ms later it does not. Why the reader never takes the field afterwards is not isolated.
  Present since R-01 after (`R-01/after/round.json`, same paths), so R-01's C1 was never measured by
  keyboard on a phone.
- **T13** `core/bot.ts` `onThisPage()` with the search branch: a result on the page the reader is on
  is dropped, and if none is left the path says "Escape (no result)". Shown by `plays/request-log-size--390x844--*--pointer.json` step 8: the search for "request records" has one result, on the CLI reference (`spot.json`), the page the reader is on. A reader on the truth page cannot use the search to jump within it, and the path calls a found result "no result".
- **T14** `cli/round.ts` per-task table: "tabs from the keyboard play at the same viewport and theme"
  prints a keyboard play's tabs and `reached` beside a pointer path the keyboard play did not
  take. Shown by `report.md` row `http-call 390x844`: pointer 5 actions, tabs 72 from a 24-action
  keyboard path. The keyboard path is in no table.
- **T15** `scenarios/tasks.json` against `scenarios/cards.json`: a task's words are not checked
  against its card's twists. `request-log-size` words include `--observe-requests`, which the
  twist says the persona does not know; R01-caller's twist on the Observe page's words is false
  (§ 2.6). Nothing in `core/findings.test.ts` or `units.test.ts` checks a twist.
- **T16** `cli/round.ts` writes no "did the last change set move its metrics" section, though the
  round's README asks for it (triage rule 4) and `last` is already read. Shown by `report.md`
  against `README.md`.

## § 4 Caveats to send with the round

- The run is red: `request-log-size` at 390x844 reached 0 of 4, with no mark and no finding. The
  cause is UXF-009 (answer cell off screen to the right), as the README says. Its keys are the
  failure charge.
- The ratchet fails as measured: 6 missing (the new task), 2 improved-not-taken that are tab-count
  noise. Nothing is worse.
- `observe` and `topology` are reached in 1 action at 1280 since R-01's C2; their keys stay at the
  31 / 93 / 400 charge because 390 is unreached.
- By keyboard on a phone the reader never searches; the keyboard numbers at 390 describe a site
  without search.
- Keyboard tab counts vary by 1 to 10 between runs on the same path; single cells are not
  comparable at tolerance 0.
- The `release` truth (changelog only) no longer matches the release sentence the site now shows.
- The measured build is `.out/R-02-build/`, not `website/build` (rebuilt after the run).
- No people, no questionnaire, no recordings. L4 and L5 not run.

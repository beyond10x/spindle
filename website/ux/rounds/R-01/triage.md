# R-01 triage

Applied by rule (SKILL.md § 3.3): `priority = severity weight × frequency × confidence ÷ cost`;
S1 9, S2 3, S3 1; cost S 1, M 2, L 4; confidence 2 where two sources agree.

## Caveats

- Base: `origin/main` moved to 82faeb87 (wave 18) under R-00's change set. Its new section on HTTP
  calls and the topology clause of the `--observe` row raise five reading-load values
  (`page.concepts-environment-and-admission` words 511 → 818, long paragraphs 0 → 2, longest
  paragraph 68 → 121; `page.reference-cli` words 3358 → 3549, long paragraphs 11 → 12). Content
  main added, no UI change: taken into the baseline under the decision id `R-01-base-82faeb87`.
- Frequency is counted in the unit the finding lives in: page findings over the 48 gating page
  checks, task findings over the 7 tasks (5 R-00 tasks and the 2 of the rotating card R01-caller).
- Confidence 2 = a measurement (an L2 page check or an L3 play) and a review agent agree. The
  report review (`review-report.md`) confirmed the topology failure, the kicker count (UXF-016) and
  UXF-007; the heuristic review (`review-heuristics.md`) filed UXF-013 to UXF-016 and confirmed
  UXF-001 and UXF-006 to UXF-012.
- The trend's action and look sums jump from 71 to 126 and 185 to 331 only because two tasks were
  added; on the 5 R-00 tasks they are unchanged (71, 185), and 62 of the 126 are charges for the
  unreached topology plays.
- Report-review tool defects fixed in this round: T8 (layout shift rounded twice: now rounded once,
  to 3 places, in `pageMetrics`), T9 (the `levels` text named R-00 and claimed the reviews before
  they ran). Open for R-02: T1 (an unreached play is not charged for detours), T2 (worst-of hides
  movement at the viewports that reach), T3 (the reader reads on only after the first word), T4
  (code-block buttons match by section heading), T5 (each play runs once: a timing-dependent path
  fails the ratchet at tolerance 0, seen on `http` `keyboard.tabs` 25/35 and `install` 1/2
  actions), T6, T7, T10, T11, and R-00's T2, T3, T8, T9.
- UXF-011's cost is L (the shared `@beyond10x/docs-system` type scale); it ties UXF-009 at 0.75 and
  takes the fifth place by rule 2: its pages carry both anchor cards' tasks (the install command
  is a 13.1 px code block), UXF-009's carry none (observe and topology are rotating cards). Built
  as a local override in `custom.css`; the shared package should take it over.
- UXF-007 is a wrong fact. The sentence PR #38 (`docs/unslop`, not on main) already holds replaces
  it word for word, so that the later merge of #38 applies without a conflict.

## Did the last change sets move their metrics?

R-00's set against R-00 `before/` (re-measured on the R-01 base, so the numbers here are R-01's):

| round | change | target metric | before | after (R-01 measure) | moved |
|---|---|---|---|---|---|
| R-00 | C1 links underlined (UXF-003) | `axe.light` + `axe.dark`, 7 pages | 46 nodes | 0 | yes |
| R-00 | C2 theme toggle on a phone (UXF-002) | gate-2 "theme toggle" at 390x844 | 24 misses | 0 | yes |
| R-00 | C3 sidebar link to serving over HTTP (UXF-004) | `http` gate 4; `screen_only.actions` | 19 | 3 | yes |
| R-00 | C4 sidebar link "run --observe" (UXF-005) | `observe` gate 4 at 1280x800; detours | not reached; 2 | reached, 7 actions; 1 (worst-of, 390x844 unreached) | yes at 1280x800; at 390x844 held by UXF-009 |
| R-00 | C5 offline search (UXF-001) | gate-2 "search" | waited for the decision | – | built in R-01 |

Side effect found in R-01: C3 and C4 are sidebar links, and the shared doc header counts them as
pages: the concept kickers read "n of 8" over six pages (UXF-016). Fixed in this round (C3 below).

## Ranking

| finding | sev | frequency | conf | cost | priority | in the set |
|---|---|---|---|---|---|---|
| UXF-001 no search | S2 | 48/48 = 1.00 | 2 | M | 3.00 | C1, design decision (answered yes) |
| UXF-013 "run --observe" lands five screens above its row | S2 | 2/7 = 0.29 | 2 | S | 1.71 | C2 |
| UXF-016 concept kickers count to 8 | S3 | 24/48 = 0.50 | 2 | S | 1.00 | C3 |
| UXF-007 release state contradicts the changelog | S2 | 1/7 = 0.14 | 2 | S | 0.86 | C4 |
| UXF-011 key data under 14 px | S2 | 48/48 = 1.00 | 1 | L | 0.75 | C5 (tie, anchor cards) |
| UXF-009 table answers off screen on a phone | S2 | 12/48 = 0.25 | 2 | M | 0.75 | no (6th) |
| UXF-012 two counts of six | S3 | 32/48 = 0.67 | 1 | S | 0.67 | no |
| UXF-010 file paths fill the phone's first screen | S3 | 18/48 = 0.38 | 1 | S | 0.38 | no |
| UXF-006 release state costs 17 to 26 actions | S3 | 1/7 = 0.14 | 2 | S | 0.29 | no |
| UXF-014 Observe page does not name the topology endpoint | S3 | 1/7 = 0.14 | 2 | S | 0.29 | no |
| UXF-015 outbound HTTP calls not in the navigation | S3 | 1/7 = 0.14 | 2 | S | 0.29 | no |
| UXF-008 sidebar leaves at the page end | S3 | 22/48 = 0.46 | 2 | L | 0.23 | no |

No S1 finding.

## Change set (5)

One task per round under one standing story; this repository's planning store has none for the
docs site yet, so the set is recorded here and in the round report. Acceptance: each regression
asset red on the R-01 site as measured (`.out/R-01-build`) and green after, and the target metric
moved.

| id | finding | change | file | regression asset | target metric |
|---|---|---|---|---|---|
| C1 | UXF-001 | offline search in the navbar (`@easyops-cn/docusaurus-search-local` 0.55.3); on a phone the right navbar items leave room for the field; the reader searches when nothing on screen carries a task word | `website/package.json`, `website/docusaurus.config.ts`, `website/src/css/custom.css`, `ux/core/bot.ts`, `ux/adapter.ts` | `e2e/findings.spec.ts::UXF-001`; gate-2 "search" without its mark | gate-2 "search" 48 → 0 misses; `screen_only.actions` of `http-call`, `environment` |
| C2 | UXF-013 | each option row of a reference table gets an anchor (`run--observe`), and the sidebar link lands on it | `website/src/rehype/option-anchors.ts`, `website/docusaurus.config.ts`, `website/sidebars.ts`, `custom.css` (`scroll-margin-top`) | `e2e/findings.spec.ts::UXF-013` | gate 4 of `topology` at 1280x800; `screen_only.actions` of `observe` at 1280x800 (7) |
| C3 | UXF-016 | the two sidebar shortcuts become HTML items, which the doc header does not count | `website/sidebars.ts` | `e2e/findings.spec.ts::UXF-016` | concept kickers "n of 6", positions 1 to 6 |
| C4 | UXF-007 | Getting started names the GitHub releases instead of "no release yet" (PR #38's sentence) | `website/docs/getting-started.md` | `e2e/findings.spec.ts::UXF-007` | the release sentence agrees with `CHANGELOG.md` |
| C5 | UXF-011 | table cells, code blocks, status chips and the landing status strip at 14 px | `website/src/css/custom.css` | `e2e/findings.spec.ts::UXF-011` | text under 14 px in those elements on 5 pages: 0 |

Built failing-first: the 7 R-01 assertions of `e2e/findings.spec.ts` (UXF-001 ×2, UXF-007, UXF-011, UXF-013 ×2, UXF-016) ran red on the R-01 site as measured (`UX_BUILD=.out/R-01-build`, 7 failed) and green on the changed site (all 15 finding assertions, R-00's included, passed). R-00's UXF-005 phone assertion now opens the menu itself: the reader may search before it opens the menu.

Gate and ratchet: the re-measure (`after/`) is green, 208 of 208 (gating and measured-only), 0 unmarked failures. Found and fixed during the build: C1 covered the phone theme toggle (the field was absolutely placed; now in the navbar's flow), and the field's shortcut hint failed axe contrast in light at 1280x800 (13 nodes, now the body text colour). The ratchet took the after values in: 11 keys worse, each with its decision id (5 `R-01-base-82faeb87`, 6 `R-01`: `keyboard.tabs` of install, http and environment +1 to +5, the search field being one more Tab stop; `release` detours 1 → 2; Getting started +18 words, C4; Capabilities +13 words at 390x844, the table guidance shown once C5's 14 px cells overflow), every improvement taken in, 0 violations.

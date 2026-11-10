# R-00 triage

Applied by rule (SKILL.md § 3.3): `priority = severity weight × frequency × confidence ÷ cost`;
S1 9, S2 3, S3 1; cost S 1, M 2, L 4; confidence 2 where two sources agree.

## Caveats

- R-00 is the first round: no earlier change set, so the table "did the last change sets move
  their metrics?" has no rows. The movements of this round's change set are in `after/report.md`
  against `before/report.md`.
- Frequency is counted in the unit the finding lives in: page findings over the 48 gating page
  checks (12 pages × 2 viewports × 2 themes), task findings over the 5 tasks.
- Confidence 2 = a measurement (L2 page check or L3 play) and the heuristic review
  (`review-heuristics.md`) agree. The review confirmed UXF-001 to UXF-008; its own four findings
  have one source each, except UXF-009, which the L3 observe play at 390x844 confirms.
- The report review (`review-report.md`) found no wrong number and eleven tool defects. Fixed
  before the `before/` measurement: T1 (the pointer parks off the page after a click), T4 (an
  unreached play counts as the cap: 31 actions, glance 3 each, 400 Tab presses; the gate-4 marks
  match "not reached" only), T5 (the sidebar mark matches the end-state miss only), T6 (the report
  says how many verdicts were red as run, and which build was measured), T7 (`always_visible.misses`
  sums over the viewports; the trend's axe column says light + dark), T11 (the reporter writes
  beside `UX_OUT`). Open for R-01: T2 (in-view test takes any line of a block), T3 (the reader
  never scrolls a table sideways), T8 (only controls are matched; prefix match), T9 (keyboard
  scrolling is scripted), T10 (worst-page tie-break).
- UXF-011's cost is taken as L, not the reviewer's M: the type scale (chips, tables, code) is set
  by the shared `@beyond10x/docs-system` package; a local override would fight every update of it.

## Did the last change sets move their metrics?

| round | change | target metric | before | after | moved |
|---|---|---|---|---|---|
| – | none (R-00 is the first round) | – | – | – | – |

## Ranking

| finding | sev | frequency | conf | cost | priority | in the set |
|---|---|---|---|---|---|---|
| UXF-003 links by colour only | S2 | 28/48 = 0.58 | 2 | S | 3.50 | C1 |
| UXF-001 no search | S2 | 48/48 = 1.00 | 2 | M | 3.00 | C5, design decision (waits) |
| UXF-002 theme toggle hidden on a phone | S2 | 24/48 = 0.50 | 2 | S | 3.00 | C2 |
| UXF-004 HTTP not findable | S2 | 1/5 = 0.20 | 2 | S | 1.20 | C3 (tie with UXF-005; anchor card B first) |
| UXF-005 run --observe not findable | S2 | 1/5 = 0.20 | 2 | S | 1.20 | C4 |
| UXF-009 table answers off screen on a phone | S2 | 12/48 = 0.25 | 2 | M | 0.75 | no (6th) |
| UXF-011 key data under 14 px | S2 | 48/48 = 1.00 | 1 | L | 0.75 | no |
| UXF-012 two counts of six | S3 | 32/48 = 0.67 | 1 | S | 0.67 | no |
| UXF-007 release state contradicts the changelog | S2 | 1/12 = 0.08 | 2 | S | 0.50 | no: a fact; for the operator |
| UXF-006 release state costs 17 to 26 actions | S3 | 1/5 = 0.20 | 2 | S | 0.40 | no |
| UXF-010 file paths fill the phone's first screen | S3 | 18/48 = 0.38 | 1 | S | 0.38 | no |
| UXF-008 sidebar leaves at the page end | S3 | 22/48 = 0.46 | 2 | L | 0.23 | no |

No S1 finding.

## Change set (5)

One task per round under one standing story; this repository's planning store has none for the
docs site yet, so the set is recorded here and in the round's report. Acceptance: each regression
asset red on the R-00 site and green after, and the target metric moved.

| id | finding | change | file | regression asset | target metric |
|---|---|---|---|---|---|
| C1 | UXF-003 | links inside the article underlined | `website/src/css/custom.css` | `e2e/findings.spec.ts::UXF-003` | `axe.light`, `axe.dark` of 7 pages |
| C2 | UXF-002 | the navbar's own theme toggle shown below 997 px | `website/src/css/custom.css` | `e2e/findings.spec.ts::UXF-002` | `always_visible.misses`; gate-2 "theme toggle" |
| C3 | UXF-004 | sidebar link "Serving ingress.network over HTTP" under Concepts, to the Capabilities section | `website/sidebars.ts` | `e2e/findings.spec.ts::UXF-004` | gate 4 of `http`; `screen_only.actions` of `http` |
| C4 | UXF-005 | sidebar link "run --observe" under Concepts after Observe, to the CLI reference's `run` section | `website/sidebars.ts` | `e2e/findings.spec.ts::UXF-005` | gate 4 of `observe` at 1280x800; `screen_only.detours` of `observe` |
| C5 | UXF-001 | offline search in the navbar | – | gate-2 "search" | gate-2 "search"; waits for `decision.md` |

Built failing-first: the 8 assertions of `e2e/findings.spec.ts` ran red on the R-00 site
(`before/`, 8 failed) and green on the changed site (`after/`).

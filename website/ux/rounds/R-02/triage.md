# R-02 triage

Applied by rule (SKILL.md § 3.3): `priority = severity weight × frequency × confidence ÷ cost`;
S1 9, S2 3, S3 1; cost S 1, M 2, L 4; confidence 2 where two sources agree.

## Caveats

- Base: unchanged from R-01 (`origin/main` at 82faeb87 with R-00's and R-01's sets, uncommitted).
  R-02's measure is R-01's `after/` site plus the rotating task `request-log-size`; the trend's
  action and look sums rise from 101 to 132 and 254 to 347 for that task alone (31 actions and 93
  looks, 26 and 56 of them in the 390x844 plays that do not reach).
- Frequency is counted in the unit the finding lives in: page findings over the 52 gating page
  checks, task findings over the 8 tasks.
- Confidence 2 = a measurement and a review agent agree. The heuristic review
  (`review-heuristics.md`, 5 pass, 9 partial, 2 fail) filed UXF-017 to UXF-019; the report review
  (`review-report.md`) confirmed the four unmarked red verdicts of `request-log-size` at 390x844
  (UXF-009) and found 13 omissions and tool defects T12 to T16.
- Tool defects fixed in this round: T12 (the keyboard reader never searched at 390x844: the focused
  field has role `combobox`, not `searchbox`; `isSearchField` now takes either) and T5's
  repeatability (a state signature now counts Tab stops, so a menu that opens without changing the
  text is no longer a loop). Both change the keyboard plays' paths; their ratchet keys carry the
  decision id `R-02`.
- The cards' twists were wrong in two places and are corrected: R01-caller's Observe page carries
  "topology" once (the fixture UI's screen), and R02-phone-operator's words put "request records"
  first and the flag name second.
- Open tool defects for R-03: T1 to T4, T6, T7, T10, T11, T13 to T16 (`review-report.md`). The
  `release` truth accepts the changelog only, so its play cannot end on Getting started's release
  sentence (T15).

## Did R-01's change set move its metrics?

R-01's `after/` against R-01's measure, and R-02's measure for the viewports R-01 did not play:

| change | target metric | before | after | moved |
|---|---|---|---|---|
| C1 offline search (UXF-001) | gate-2 "search" | 48 misses (marked) | 0 of 52 | yes |
| C1 | `screen_only.actions` of `http-call`, `environment` (pointer, 1280x800) | 24, 6 | 5, 5 | yes; at 390x844 keyboard only after T12 (this round) |
| C2 row anchors (UXF-013) | gate 4 of `topology` at 1280x800; `observe` actions at 1280x800 | not reached; 7 | reached in 1; 1 | yes |
| C3 kickers (UXF-016) | concept kickers "n of 6", 1 to 6 | "n of 8", gaps | "n of 6", 1 to 6 | yes |
| C4 release sentence (UXF-007) | the sentence agrees with `CHANGELOG.md` | "no release yet" | GitHub releases | yes; the pasted 0.6.0 output stays (UXF-018) |
| C5 14 px key data (UXF-011) | text under 14 px in key data, 5 pages × 4 | 1 or more per page | 0 | yes; side effects: the run options table 43 → 133 px wider than its column at 1280x800 (UXF-019), two more tables overflow at 390x844 |

## Ranking

| finding | sev | frequency | conf | cost | priority | in the set |
|---|---|---|---|---|---|---|
| UXF-009 table answers off screen on a phone | S2 | 3/8 = 0.38 (observe, topology, request-log-size) | 2 | M | 1.13 | C1 |
| UXF-017 search lands on the section, not the row | S2 | 1/8 = 0.13 | 2 | M | 0.38 | C2 |
| UXF-019 run options table wider than its column at 1280 | S3 | 3/8 = 0.38 | 1 | S | 0.38 | C3 |
| UXF-012 two counts of six | S3 | 8/52 = 0.15 | 2 | S | 0.31 | C4 |
| UXF-014 Observe page does not name the topology endpoint | S3 | 1/8 = 0.13 | 2 | S | 0.25 | no (tie, prose) |
| UXF-006 release state costs 17 to 26 actions | S3 | 1/8 = 0.13 | 2 | S | 0.25 | no (tie) |
| UXF-008 sidebar leaves at the page end | S3 | 22/52 = 0.42 | 2 | L | 0.21 | no |
| UXF-015 outbound HTTP calls not in the navigation | S3 | 1/8 = 0.13 | 1 | S | 0.13 | no (3 actions at 1280 by search) |
| UXF-010 file paths fill the phone's first screen | S3 | 18/52 = 0.35 | 1 | L (re-costed) | 0.09 | no |
| UXF-018 search answers "version" with 0.6.0 | S3 | 0/8 | 1 | S | 0 | no; fixed by PR #38 in this branch's unslop pass |

No S1 finding.

- UXF-012 re-counted: R-00 counted every page with a kicker (32/48); the asset shows the defect on
  2 pages (Overview "1 of 6", Getting started "2 of 6"), 8 of 52 checks.
- The fifth place ties UXF-014 and UXF-006 at 0.25 and stays empty: UXF-014 is prose on the
  Observe page, which PR #38 and the unslop pass edit; UXF-006 is mostly the `release` truth
  accepting only the changelog (T15).
- UXF-010 re-costed L: the source list is the shared `@beyond10x/docs-system` doc header; a local
  fix means swizzling that component. Left for the shared package.

## Change set (4)

Acceptance: each regression asset red on the R-02 site as measured (`.out/R-02-build`) and green
after, and the target metric moved.

| id | finding | change | file | regression asset | target metric |
|---|---|---|---|---|---|
| C1 | UXF-009 | below 997 px a table whose cells carry their column's header (`data-label`, set at build) shows each row as label and value pairs | `website/src/rehype/table-labels.ts`, `website/docusaurus.config.ts`, `website/src/css/custom.css` | `e2e/findings.spec.ts::UXF-009` (both themes) | gate 4 and gate 5 of `observe`, `topology`, `request-log-size` at 390x844: 12 red or marked → 0 |
| C2 | UXF-017 | a page opened from the search moves on to the anchored table row that carries every searched word, in the hit's section first | `website/src/client/search-row.ts`, `website/docusaurus.config.ts` | `e2e/findings.spec.ts::UXF-017` (4 viewports) | `request-log-size` at 390x844 reached; its actions and looks |
| C3 | UXF-019 | a path in a table cell may wrap after each `/` (`<wbr>`, set at build) | `website/src/rehype/code-breaks.ts`, `website/docusaurus.config.ts` | `e2e/findings.spec.ts::UXF-019` (both themes) | docs tables wider than their column at 1280x800: 1 → 0 |
| C4 | UXF-012 | Overview and Getting started name their set ("Start") instead of counting "1 of 6", "2 of 6" | `website/docs/index.md`, `website/docs/getting-started.md` (front matter `kicker`) | `e2e/findings.spec.ts::UXF-012` | kickers with a bare count: 2 → 0 |

Built failing-first: the 7 new R-02 assertions of `e2e/findings.spec.ts` (UXF-009 ×2, UXF-017 ×2, UXF-019 ×2, UXF-012) ran red on the R-02 site as measured (`UX_BUILD=.out/R-02-build`, 7 failed) and green on the changed site (all 22 finding assertions passed). The first green run failed UXF-013 at 1280x800: C3 narrows the meaning column (421 → 328 px), so the `--observe` row is 1474 px tall and its first cell cannot be wholly on an 800 px screen; UXF-013 and UXF-017 now assert the option's name in view. Site render steps have unit tests (`site.test.ts`, 3 tests; `npm test` 47 pass).

Gate and ratchet: the first re-measure (`after/`) was 221 of 223 green; red were gate 5 (keyboard) of `request-log-size` at 390x844 in both themes. The pointer plays reach the row in 14 actions; the keyboard reader walks the article with Tab after the phone menu closes (279 Tabs) and is left below the answer. Filed as UXF-020 (S2, open) with a mark on those two verdicts; the second re-measure is 223 of 223 green, 0 unmarked failures. The four UXF-009 marks are removed (they covered nothing). The ratchet took the after values in: 4 keys worse, each `keyboard.tabs`, under the decision id `R-02` (environment 15 → 90, http-call 72 → 93, install 19 → 21, release 131 → 215: T12 makes the keyboard reader use the search, and the Tab walk to the field is UXF-020), 22 improvements taken in (http `keyboard.tabs` 30 → 27; observe and topology 31 → 2 actions, 93 → 2 looks, 400 → 27 and 32 Tabs; 9 pages 2 to 104 words fewer, the table guidance line gone where tables fit), 6 new keys for `request-log-size`, 0 violations.

## After the round: the unslop pass (decision `R-02-unslop`)

The docs pass brought in PR #38 (`docs/unslop`) and edited main's later text (the section on HTTP
calls, README's topology paragraph). It closes UXF-018 (Getting started's output re-run at 0.9.0;
asset red on `.out/R-02-build`, green after). `task ux:docs` on the result: 146 of 146 green; the
ratchet took in 17 improvements (long paragraphs on 6 pages 1 to 2 fewer, the longest paragraph
on 6 pages down to 67 to 84 words, Capabilities 1049 → 1042 words, Deployment targets 485 → 480,
Observe `layout_shift` 0.005 → 0, `keyboard.tabs` of install 21 → 17 and release 215 → 170) and 12
worse values under `R-02-unslop`: words on 9 pages (+7 to +102; PR #38 splits semicolon chains
and gives each sentence its subject, as the skill reports for its sentence passes; landing and
Status +14 from the longer Bindings line in `status.rs`), Getting started's longest paragraph
74 → 77 words, and `keyboard.tabs` of http 27 → 29 and observe 27 → 28 (the edited pages carry
more Tab stops on those paths).

Tool change after the pass (T5): two `task ux:docs` runs on one site gave `keyboard.tabs` release 170 and 214, observe 28 and 30, install 17 and 21 (light and dark of one run differ too, 210 and 214, on the same path), so a tolerance of 0 failed the ratchet on noise. `keyboard.tabs` now holds a 30 % band both ways (`BAND_KEYS` in `core/metrics.ts`, `lowest` in `core/ratchet.ts`, unit test in `core/ratchet.test.ts`); every other key is unchanged. The baseline values stay; their tolerance is 0.3.

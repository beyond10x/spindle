# R-03 triage

Applied by rule (SKILL.md § 3.3): `priority = severity weight × frequency × confidence ÷ cost`;
S1 9, S2 3, S3 1; cost S 1, M 2, L 4; confidence 2 where two sources agree.

## Caveats

- Base moved: R-02 ran on 82faeb87, R-03 on 8591d014 with the fourth unslop pass uncommitted
  (`README.md`). Page keys measure `main`'s new content as well as the unslop pass. The ratchet
  against R-02's baseline had 27 violations before any change (`review-report.md` § 1).
- `task ux:docs` is not in CI, so three regressions reached `main` unseen: UXF-018's test (Getting
  started showed 0.10.0 after the 0.18.0 release), `request-log-size` at 1280x800 (UXF-022) and the
  `release` keyboard plays. The round task exits 0 with failed Playwright tests (T17), so the R-03
  report showed none of the 10 failures.
- No frozen build of the measured site (T10): the evidence of the red state is the measure log,
  10 failed of 225.
- The heuristic review (`review-heuristics.md`, 7 pass, 8 partial, 1 fail) filed UXF-023 to
  UXF-025. UXF-024 is the measure's UXF-022 found independently: confidence 2.
- The report review (`review-report.md`) found T17 to T19 and confirmed T1 to T4, T6, T7, T10,
  T11, T13 to T16 still open (T12 fixed).
- Frequency counts tasks (8) for task findings.

## Did R-02's change set move its metrics?

| change | target metric | R-02 after | R-03 | held |
|---|---|---|---|---|
| C1 phone tables (UXF-009) | gate 4 and 5 of `observe`, `topology` at 390x844 | reached, 2 actions | reached, 2 actions | yes |
| C2 search moves on to the row (UXF-017) | `request-log-size` at 390x844 reached | reached (pointer) | not reached | no: base moved, UXF-022 |
| C3 `<wbr>` after `/` in cells (UXF-019) | tables wider than their column at 1280x800: 0 | 0 | 0 | yes; side effect UXF-023: each such cell code renders as a code block |
| C4 kickers (UXF-012) | kickers with a bare count: 0 | 0 | 0 | yes |

## Ranking

| finding | sev | frequency | conf | cost | priority | in the set |
|---|---|---|---|---|---|---|
| UXF-022 / UXF-024 request-log row not reached at 1280x800 | S2 | 1/8 | 2 | S | 0.75 | C2 |
| UXF-023 code in table cells renders as code blocks | S2 | 1/8 | 2 | S | 0.75 | built, taken out (below) |
| UXF-006 release state takes 17 to 26 actions; its keyboard plays fail gate 5 | S3 | 1/8 | 2 | S | 0.25 | C3 (gate red) |
| UXF-018 Getting started shows 0.10.0, latest is 0.18.0 | S3 | 0/8 | 1 | S | 0 | C1 (its asset is red) |
| UXF-020 phone keyboard walks the article after the menu closes | S2 | 1/8 | 1 | L | 0.09 | no: shared theme package |
| UXF-008 sidebar leaves at the page end | S3 | 22/52 | 2 | L | 0.21 | no |
| UXF-014, UXF-015 | S3 | 1/8 | 1 to 2 | S | 0.13 to 0.25 | no |
| UXF-025 desktop "On this page" after the article in Tab order | S3 | 1/8 | 1 | M | 0.06 | no |
| UXF-010 file paths fill the phone's first screen | S3 | 18/52 | 1 | L | 0.09 | no |

No S1 finding. C1 and C3 enter for a red gate, ahead of their priority.

## Change set (3)

| id | finding | change | file | regression asset | target metric |
|---|---|---|---|---|---|
| C1 | UXF-018 | Getting started prints `b10x-spindle 0.18.0`; the release steps name that line | `website/docs/getting-started.md`, `AGENTS.md` | `e2e/findings.spec.ts::UXF-018` | its assertion red → green |
| C2 | UXF-022, UXF-024 | the Observe request-log section gives the default 256 and links the `--observe-requests` row; row ids are registered with the broken-anchor check so a Markdown link may land on a row | `website/docs/concepts/observe.md`, `website/src/theme/MDXComponents.tsx` | `e2e/findings.spec.ts::UXF-022 and UXF-024` | gate 4 and 5 of `request-log-size` at 1280x800: 6 red → 0 |
| C3 | UXF-006 | the Status page links `CHANGELOG.md` under its first paragraph | `crates/spindle-docs/src/status.rs`, `website/docs/status.md` (generated) | the L3 `release` plays (gate 4, gate 5) | `release` actions 18 and 26; keyboard plays 3 red → 0 |

Tool change (T3, part): while reading a page, the reader follows a link in the text whose name
carries a task word (`readingPick` in `core/bot.ts`, unit test in `core/units.test.ts`). Before,
it scrolled past such a link to the page end. This changes play paths; their ratchet keys carry
the decision id `R-03`.

The UXF-020 mark is removed: with C2 no gated play shows it. UXF-020 stays open.

UXF-023 was built as a fourth change and taken out. Code whose children are text and `<wbr>`
rendered as inline code (`src/theme/MDXComponents.tsx`). Its asset went red (54 code blocks in
cells) → green. The L3 play `environment` at 390x844 then failed 8 of 26 runs, against 1 of 20
without the change; an A/B at one host load gave 6 of 8 against 8 of 8. UXF-023 is open, with the
A/B in its record, for a diagnosis before the next attempt.

## Result

Re-measure (`after/`): 226 of 226 Playwright tests green; gate 4 32 of 32 and gate 5 (keyboard)
32 of 32, against 28/4 and 25/5/2 in the measure. Trend sums: actions 79 → 35, looks 180 → 40.

| task | metric | measure | after |
|---|---|---|---|
| `release` | actions (worst cell) | 26 | 3 |
| `release` | looks | 62 | 4 |
| `release` | path at 1280x800 | Status → search → 13 scrolls → footer | Status → `CHANGELOG.md` |
| `request-log-size` | reached | 2 of 8 plays | 8 of 8 |
| `request-log-size` | actions (worst cell) | 31 | 10 |

Ratchet: `task ux:docs:baseline DECISION=R-03` took 16 keys in under the decision id: words and
paragraph keys of 8 pages that grew on `main` since R-02 (Environment and admission 840 → 2869
words, the CLI reference 3445 → 4953). The play keys of `release` and `request-log-size` improved and came in without one, as did the unslop
pass's (Observe and Specification long paragraphs 1 → 0 and
2 → 1).

The Playwright test timeout is 60 s (`playwright.config.ts`, was 180 s), per the test budget of
the repository rules: no single test over 60 s.

Built failing-first: UXF-022's assertion has no link to find on the R-03 page (red by
construction); C1 and C3 turned the measure's red UXF-018 test and `release` keyboard plays green;
`readingPick`'s unit test ran red (no export) before the change.

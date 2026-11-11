# R-01 heuristic review

8 pass, 4 partial, 4 fail

Inputs: `report.md`, the four contact sheets and four worst shots in `shots/`, the raw run in
`.out/R-01-run/` (48 page checks, 56 plays, verdicts, per-page shots), the cards, the operator notes
and `references/heuristics.md`. No recordings exist in this harness, so evidence is a play file
(`plays/<task>--<viewport>--<theme>--<mode>.json`, under `.out/R-01-run/`) or a screenshot. Extra
shots `shots/review-*.png` come from the frozen build `.out/R-01-build/`, served on port 4331, with
a probe whose output is `.out/review-1-R-01/probe.json`.

| # | heuristic | verdict | evidence |
|---|---|---|---|
| 1 | One next thing | pass | Landing has one filled button, "Get started" (`.out/R-01-run/shots/landing--1280x800--light.png`); every docs page ends in a Previous/Next pager (probe `kickers.*.pager`). |
| 2 | State in words | pass | Status chips read "SHIPPED"/"PLANNED", not only a colour (`contact--390x844--light.jpg`, status frame); axe finds no colour-only nodes (all 48 `pages/*.json` have `axe: []`). |
| 3 | Nothing required hidden | fail | No search on any page, 48 of 48 checks (UXF-001, `pages/*.json` "search absent"); on a phone the answer column of every table is past the right edge (UXF-009, `review-topology-cell--390x844--light.png`); observe at 390x844 not reached in 4 of 4 plays (`plays/observe--390x844--light--pointer.json`). |
| 4 | Stable layout | pass | `layout_shift` 0 on 46 of 48 checks, 0.0005 on Getting started at 390x844 (`pages/getting-started--390x844--light.json`). |
| 5 | Provenance of every value | pass | Every docs header lists its source files, CLI says "Generated from crates/spindle-cli/src/cli.rs", Status says "Checked against main ... on 2026-11-04" (`worst--reference-cli--390x844--light.png`, `contact--390x844--light.jpg`). |
| 6 | Errors cheap | pass | A mistyped docs URL shows "Page not found" with "Go to the documentation" and "Back to the home page", one action each (`review-404--1280x800--light.png`, `review-404--390x844--light.png`). |
| 7 | Words the user can say | pass | Navigation labels are page titles or the CLI's own tokens ("run --observe", "ingress.network"), readable as typed (`review-kicker-of-8--1280x800--light.png`, `worst--reference-cli--1280x800--dark.png`). |
| 8 | The reader's time respected | fail | release 17 to 26 actions (UXF-006, `plays/release--390x844--light--pointer.json`); http-call 10 to 24 actions, 181 tabs (UXF-015, `plays/http-call--1280x800--light--keyboard.json`); topology not reached in 8 of 8 (UXF-013, `plays/topology--1280x800--light--pointer.json`); on a phone the file list pushes the first sentence down to 514 to 802 px (UXF-010, `contact--390x844--light.jpg`). |
| 9 | Keyboard and pointer parity | pass | Keyboard reaches what the pointer reaches in 28 of 28 task, viewport and theme pairs (`plays/*--keyboard.json` vs `*--pointer.json`); a 2 px focus ring on every stop, both themes (`review-focus--1280x800--light.png`, `review-focus--1280x800--dark.png`); a skip link is the first stop. |
| 10 | Recognition over recall | partial | The sidebar lists every page (`review-httpcall-anchor--1280x800--light.png`), but with no search a reader must already know that the topology endpoint is a CLI flag and that HTTP calls live under Environment (UXF-001, UXF-014, `plays/topology--390x844--light--pointer.json`). |
| 11 | Calm by default | pass | The only animation is the landing's status strip growing once (`b10x-grow`, 1 iteration, probe `motion-*`); no alert, banner or looping motion on any frame of the four contact sheets. |
| 12 | One state vocabulary | partial | Status words agree between Status and landing ("shipped"/"planned"); the page-position labels do not: "1 of 6", "2 of 6", then "Concepts · 1 of 8" to "7 of 8" with no 3 or 8 (UXF-012, UXF-016, `review-kicker-of-8--1280x800--light.png`). |
| 13 | A wrong cue recovered in one action | fail | "run --observe" opens the `run` heading 4244 px above the row; every topology play turns back to Observe (UXF-013, `review-topology-landing--1280x800--light.png`, `plays/topology--1280x800--dark--pointer.json`); the sidebar's "HTTP" leads http-call readers to inbound serving, 3 links to recover (UXF-015, `plays/http-call--1280x800--light--pointer.json`). |
| 14 | Unmistakable end state | partial | Heading anchors put the heading 177 px from the top, below the 74 px navbar, at 1280x800 and on a phone, the TOC collapsible out of the way (`review-httpcall-anchor--1280x800--light.png`, `review-tocjump--390x844--light.png`); a link named for a flag ends on another heading (UXF-013), and on a phone the answer cell ends off screen (UXF-009, `review-topology-cell--390x844--light.png`). |
| 15 | Readable in every theme; key data at 14 px | fail | Both themes pass contrast (axe, 48 of 48), but key data is under 14 px: the `--observe` row and the topology clause at 12.67 px (probe `topology-landing-*`), chips, table cells and code blocks (UXF-011, `review-topology-cell--1280x800--dark.png`). |
| 16 | Focus follows the work | partial | TOC jumps put the target in view on a phone (`review-tocjump--390x844--light.png`); the sidebar leaves the view at the end of every docs page at 1280x800 (UXF-008, `pages/concepts-observe--1280x800--light.json` end state), and the "run --observe" link puts the view five screens from the work (UXF-013). |

## Findings filed

| id | title | severity | class | sessions | cost |
|---|---|---|---|---|---|
| UXF-013 | The "run --observe" link lands on the run heading, five screens above the --observe row | S2 | cue | topology 8 of 8 (not reached), observe 1280x800 4 (7 actions), observe 390x844 4 | S |
| UXF-014 | The Observe page names topology only as a screen of the fixture UI, never the live topology endpoint | S3 | cue | topology 8 of 8 (1 to 2 detours) | S |
| UXF-015 | The only "HTTP" in the navigation is serving over HTTP; outbound HTTP calls take 10 to 24 actions | S3 | cue | http-call 8 of 8 | S |
| UXF-016 | The concept kickers count to 8 over six concept pages | S3 | copy | 24 page checks of the 6 concept pages | S |

The topology gate-4 and gate-5-keyboard failures (4 + 4 verdicts) had no finding and no
known-failing mark; UXF-013 is the one that covers them.

## Existing findings confirmed in this round

| id | status | what R-01 shows |
|---|---|---|
| UXF-001 | open | Search still absent, 48 of 48 checks. |
| UXF-002 | closed | Holds: the theme toggle is in the phone navbar on every 390x844 frame. |
| UXF-003 | closed | Holds: axe 0 serious or critical nodes on 48 of 48. |
| UXF-004 | closed | Holds: http reached in 8 of 8, 2 to 3 actions. |
| UXF-005 | closed | Holds at 1280x800 (observe reached); the link it added is the cue of UXF-013. |
| UXF-006 | open | release 17 to 26 actions, 122 to 134 tabs. |
| UXF-007 | open | Getting started still says "Spindle has no release yet" (`contact--390x844--light.jpg`). |
| UXF-008 | open | Docs sidebar out of view at the end, every docs page at 1280x800. |
| UXF-009 | open | observe at 390x844 not reached; the `--observe` meaning cell at x=383 (probe). |
| UXF-010 | open | File lists before the first sentence on every phone docs frame. |
| UXF-011 | open | Table cells and code at 12.67 px on the CLI reference (probe). |
| UXF-012 | open | Two schemes, now "of 6" and "of 8" (see UXF-016). |

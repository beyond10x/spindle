# R-00 heuristic review

6 pass, 7 partial, 3 fail

Inputs: `report.md`, `shots/` (4 contact sheets, 4 worst shots, evidence shots), `scenarios/cards.json`,
`scenarios/tasks.json`, the raw plays `.out/run/plays/*.json` and page checks `.out/run/pages/*.json`.
No recordings and no operator notes exist for this round, so evidence is a play file, a page-check
file, a shot, or the review probes. Extra shots are `shots/review-*.png` (both themes, 1280x800 and
390x844, built site served by `harness/server.ts`); the probe numbers (computed font sizes, layout
shift, animations, tab order, table columns at 390 px, header heights) are in `review-probe.json`.

| # | heuristic | verdict | evidence |
|---|---|---|---|
| 1 | One next thing | pass | Landing has one primary action, "Get started", plus two text links (`contact--1280x800--light.jpg`, landing frame; `review-probe.json` kickers_motion.landingTopActions). Every docs page ends on a Previous/Next pair (`evidence--UXF-008.png`). Task install reached in 1 action (`.out/run/plays/install--1280x800--light--pointer.json`). |
| 2 | State in words | partial | Shipped and planned are chips with the word, not only a dot (`review-status--390x844--light.png`); breadcrumbs name the place (`worst--getting-started--390x844--dark.png`). But the release state is said three ways that disagree: landing "Early", Getting started "Spindle has no release yet", changelog 0.9.0 (`worst--getting-started--1280x800--light.png`; UXF-007). |
| 3 | Nothing required hidden | fail | No search at any width (`evidence--UXF-001.png`, 48 of 48 page checks; UXF-001). Theme toggle only in the drawer below 997 px (`evidence--UXF-002.png`; UXF-002). At 390x844 the answer column of 14 tables runs off screen; the `--observe` row shows "--observe, no" and a blank (`review-cli-observe-row--390x844--light.png`, `review-cli-observe-row--390x844--dark.png`; UXF-009). |
| 4 | Stable layout | pass | Cumulative layout shift 0 on all 12 pages at 1280x800 and 390x844 in both themes, 0.001 on Getting started at 390 (`review-probe.json` pages_keyboard_anchor_404, `cls`). Contact sheets show the same frame layout per page in both themes. |
| 5 | Provenance of every value | pass | Each docs header lists its source files (`review-first-screen--capabilities--390x844--light.png`); Getting started says its output was pasted from a run on 2026-11-04 (`worst--getting-started--1280x800--light.png`); Status says what it was checked against and when (`contact--390x844--light.jpg`, status frame); footer names the build (`evidence--UXF-008.png`). |
| 6 | Errors cheap | pass | A wrong page is left in 1 action on desktop (sidebar always in view at the top, `contact--1280x800--light.jpg`) and 2 on a phone (menu, then item; the drawer opens on the current section, `review-drawer--390x844--light.png`). A bad URL lands on a 404 with two exits (`review-404--390x844--dark.png`). |
| 7 | Words the user can say | partial | Labels read out unchanged ("Getting started", "Status", "CLI"), but they do not carry the reader's words: nothing in the navigation says HTTP, serve, release or version (`.out/run/plays/http--1280x800--light--pointer.json` stuck on those words; `.out/run/plays/release--1280x800--light--pointer.json` goes to Status, which lists capabilities). "Rally" in the navbar does not say it is an example (`contact--1280x800--light.jpg`). Filed as UXF-004, UXF-006. |
| 8 | The reader's time respected | fail | http not reached in 17 to 19 actions, observe not reached in 9 to 12, release 17 to 26 actions and 132 Tab presses (`.out/run/plays/http--390x844--light--pointer.json`, `observe--1280x800--light--pointer.json`, `release--390x844--light--pointer.json`). On a phone the first sentence of 9 docs pages starts at 514 to 802 px of 844, below a 3 to 9 line list of file paths (`review-first-screen--capabilities--390x844--light.png`, `review-probe.json` header_vs_body; UXF-010). Landing is 2186 words, 1524 of them the status strip (`review-landing-full--1280x800--light.png`). |
| 9 | Keyboard and pointer parity | pass | Keyboard reached the truth exactly where the pointer did, 20 of 20 pairs (`.out/run/plays/*--keyboard.json` against `*--pointer.json`). First Tab is "Skip to main content"; a 2 px outline shows focus in both themes (`review-focus-first-tab--1280x800--light.png`, `--dark.png`); wide tables take arrow keys. The high Tab counts come from the navigation gaps in 7 and 8, not from a keyboard-only gap. |
| 10 | Recognition over recall | partial | The sidebar lists every page, the TOC every section, breadcrumbs the place (`contact--1280x800--dark.jpg`). Without search the reader must recall which page holds a term, and the HTTP answer needs the reader to know that HTTP is the `ingress.network` capability (`.out/run/plays/http--1280x800--dark--pointer.json`; UXF-001, UXF-004). |
| 11 | Calm by default | pass | One animation in the site: the landing status bars grow once (iterations 1), and none run under reduced motion (`review-probe.json` kickers_motion.anims, animsReduced 0). No banners, toasts or alerts on any frame of the four contact sheets. |
| 12 | One state vocabulary | partial | Shipped/planned are the same two words on landing, Status and concept pages (`review-landing-full--1280x800--light.png`, `review-status-table--1280x800--light.png`). Page position is not: Getting started "2 of 6", then "Concepts · 1 of 6", then nothing on Rally, CLI, Status (`worst--getting-started--1280x800--light.png`, `contact--1280x800--light.jpg`; UXF-012). The release state disagrees across pages (UXF-007). |
| 13 | A wrong cue recovered in one action | partial | Off-site links open a new tab, so a wrong off-site turn is closed in one action (`review-probe.json`, `target=_blank` on every article link). But the Observe page is the wrong cue for `run --observe` and leads off site to a design document (`.out/run/plays/observe--1280x800--light--pointer.json`, 3 detours), and there is no search to recover with (UXF-001, UXF-005). |
| 14 | Unmistakable end state | partial | Where the reader reaches the page, the truth is in view and the page ends on Previous/Next (install, environment: `.out/run/plays/install--1280x800--light--pointer.json`, `environment--390x844--light--pointer.json`). Not reached in 8 of 20 plays (http, observe); the release truth is off the site; at 390 px the observe truth would sit in an off-screen column even on the right page (`review-cli-observe-row--390x844--light.png`; UXF-004, UXF-005, UXF-009). |
| 15 | Readable in every theme; key data at 14 px or more | fail | Contrast passes in both themes (no axe color-contrast node in 48 page checks, e.g. `.out/run/pages/status--390x844--dark.json`). Key data is under 14 px: status chips 10.4 px, table headers 10.9 px, CLI option cells 12.7 px, every code block 13.1 px (the install command included), landing status details 13.3 px (`review-status-table--1280x800--light.png`, `review-probe.json` pages_keyboard_anchor_404 `small`; UXF-011). Links in prose differ by colour only (UXF-003). |
| 16 | Focus follows the work | partial | A section link lands its heading 100 px below the navbar, not under it (`review-probe.json` anchor: heading top 177, navbar bottom 74); the drawer opens on the current page (`review-drawer--390x844--dark.png`). At the end of a page the sidebar scrolls out and the reader's place in it is lost (`evidence--UXF-008.png`; UXF-008). |

## Findings filed

| id | title | severity | class | sessions | cost |
|---|---|---|---|---|---|
| UXF-009 | On a phone, the table columns that hold the answer sit off screen | S2 | layout | 12 of 24 page checks at 390x844 (6 pages x 2 themes); task observe at 390x844, 4 plays | M |
| UXF-010 | The source-file list in each page header pushes the text below the first phone screen | S3 | noise | 18 of 24 page checks at 390x844 (9 pages x 2 themes); plays http, observe, environment at 390x844 | S |
| UXF-011 | Key data is set below 14 px; status chips at 10.4 px | S2 | layout | 48 of 48 gating page checks | M |
| UXF-012 | The page-position labels count two different sixes | S3 | copy | 32 of 48 gating page checks (8 pages) | S |

Already on file and confirmed by this review, not filed again: UXF-001 (H3, H10, H13), UXF-002 (H3),
UXF-003 (H15), UXF-004 (H7, H10, H14), UXF-005 (H13, H14), UXF-006 (H7, H8), UXF-007 (H2, H12),
UXF-008 (H16). Each of these now has a second source (metric plus heuristic review) for triage's
confidence factor.

# R-03 heuristic review

Reviewer 1 (heuristics), fresh agent, 2026-11-22. Inputs: `report.md`, `shots/` with the four
contact sheets and the worst-page shots, the plays of the R-03 run (`.out/run/plays/`, read before
that directory was replaced by a later run at 22:20; their numbers match the report's tables),
`scenarios/cards.json` and `tasks.json`, findings UXF-001..021. No recordings and no operator notes
exist. Evidence is a play (file and step), a screenshot, or a probe of a copy of the R-03 build
taken before the site was rebuilt at 22:20 (helper scripts outside the repository; shots
`shots/review-*.png`).

**Verdict: 7 pass, 8 partial, 1 fail**

| # | heuristic | verdict | evidence |
|---|---|---|---|
| 1 | One next thing | pass | Landing names one primary action, "Get started" (contact sheets, landing frame); install reached in 2 actions at 1280x800 (`plays/install--1280x800--light--pointer.json`); every docs page ends on one Next link. |
| 2 | State in words | pass | Status chips say "SHIPPED" in words and the page defines shipped and planned above the table (`contact--390x844--light.jpg`, status frame); kickers say "Concepts · 3 of 6". |
| 3 | Nothing required hidden | partial | The request-log row of the CLI reference starts 1,651 px down an 800 px view after the "run --observe" link, below a 1,474 px `--observe` row of stacked code blocks (`shots/review-cli-observe-row--1280x800--light.png`, `--390x844--dark.png`): new UXF-023. The sidebar leaves the view at the end of each docs page (gate-2, 22 known-failing): UXF-008. |
| 4 | Stable layout | partial | layout_shift 0 on all 13 pages (report). Opening the search on a phone still cuts "Spindle" to "Spi…" and clips the typed query under the clear button (`shots/review-search-request-records--390x844--light.png`). Not filed, as in R-02: attention is in the field and no play lost time to it. |
| 5 | Provenance of every value | pass | Each docs header lists its source files, the CLI reference says "Generated from crates/spindle-cli/src/cli.rs", Getting started dates its pasted output (`shots/review-search-request-records--1280x800--light.png`); search "version" now answers "b10x-spindle 0.10.0" (probe). |
| 6 | Errors cheap | pass | A wrong turn in a reached play is left with one link (`plays/http--1280x800--light--pointer.json`, 1 detour, 2 actions); a query is cleared with the ✕ in the field (`shots/review-search-request-records--1280x800--light.png`). |
| 7 | Words the user can say | partial | Navigation labels read out unchanged ("Serving ingress.network over HTTP", "run --observe"). The search hit that opens the request-log row reads "…), refuses only the request that selects it: its …" (`shots/review-search-request-records--1280x800--light.png`); part of UXF-024. |
| 8 | The reader's time respected | partial | Release takes 18 actions at 1280x800 and 26 at 390x844, and search "changelog" returns nothing (probe; `plays/release--390x844--light--pointer.json`): UXF-006. On a phone the first sentence of Observe starts at 889 px and of Environment and admission at 881 px, in an 844 px view (probe; `contact--390x844--light.jpg`): UXF-010. |
| 9 | Keyboard and pointer parity | partial | install: 2 actions either way, 86 Tabs, 75 of them to reach "Build" in "On this page" after the whole article (probe): new UXF-025. Code blocks made from code in table cells add 41 Tab stops (27 on the CLI reference): UXF-023. release 1280x800 keyboard and 390x844 dark keyboard stop with "no Tab reaches link "Changelog" in 400 presses" after 25 and 154 Tabs (`plays/release--1280x800--light--keyboard.json`, step 17). A hand walk of the same path (Status, search "release", hit, scroll to the foot) reaches the footer's Changelog in 65 Tabs at 1280x800 and 58 at 390x844. Not filed: not reproduced in the UI. |
| 10 | Recognition over recall | partial | Search "topology" ranks the Observe fixture-UI sentence first and the CLI run row third (probe): UXF-014. A reader with the answer's words, "request records", gets the Observe section first, which neither has the number nor links the flag (`shots/review-observe-request-log--1280x800--light.png`): new UXF-024. |
| 11 | Calm by default | pass | No banner, toast or motion on any frame of the four contact sheets; layout_shift 0 (report). |
| 12 | One state vocabulary | pass | Concept kickers read 1 to 6 of 6 without gaps or doubles; Overview and Getting started carry "Start" with no count (`contact--390x844--light.jpg`): UXF-012 and UXF-016 hold. |
| 13 | A wrong cue recovered in one action | fail | request-log-size: not reached in all 8 plays. Each play follows "run --observe" (step 3) and lands on a row whose next row is 1.8 screens down (UXF-023). It then takes the first search hit for "request records" (step 6) to Observe › "A served run's metrics and request log", a section with no link (UXF-024), and ends stuck on `/docs` (1280x800) or `/docs/concepts/observe` (390x844) after 16 to 24 actions (`plays/request-log-size--1280x800--light--pointer.json`, `--390x844--light--pointer.json`). R-02 after reached it. |
| 14 | Unmistakable end state | pass | The CLI search hit and the row links land with the row 177 px from the top and the query words marked (`shots/review-search-cli-hit--1280x800--light.png`, `--390x844--dark.png`); observe and topology end on the `--observe` row's first line (`shots/review-cli-observe-row--1280x800--light.png`). |
| 15 | Readable in every theme, key data at 14 px | partial | axe 0 serious or critical nodes on 52 page checks (report). The light navbar's "Search" placeholder is rgb(190,195,201) on rgb(214,223,218), about 1.3:1 (probe; `worst--reference-cli--1280x800--light.png`). Not filed, as in R-02: the magnifier and "ctrl K" mark the field. |
| 16 | Focus follows the work | partial | Sidebar and search links to a row put the row in view (UXF-013 and UXF-017 hold). On a phone, closing the menu by keyboard leaves focus in the article: request-log-size 390x844 keyboard stops with "no Tab reaches button "Close navigation bar" in 400 presses" (`plays/request-log-size--390x844--light--keyboard.json`, step 22): UXF-020. The desktop "On this page" list is last in Tab order: UXF-025. |

## Existing findings

| id | status | in R-03 |
|---|---|---|
| UXF-001 | closed | holds: search field in the navbar at every gating viewport and theme (contact sheets) |
| UXF-002 | closed | holds: theme toggle in the phone navbar (`contact--390x844--*.jpg`) |
| UXF-003 | closed | holds: prose links underlined (the release link in Getting started's first sentence, `shots/review-search-request-records--1280x800--light.png`), axe 0 |
| UXF-004 | closed | holds: http 2 actions at 1280x800, 3 at 390x844 |
| UXF-005 | closed | holds: observe 1 action at 1280x800, 2 at 390x844 |
| UXF-006 | open | confirmed: 18 and 26 actions; search "changelog" returns nothing |
| UXF-007 | closed | holds: Getting started says binaries are attached to each release |
| UXF-008 | open | confirmed: gate-2 end-state miss on every docs page at desktop width (22 known-failing) |
| UXF-009 | closed | holds: phone tables stack label and value (`shots/review-search-cli-hit--390x844--dark.png`) |
| UXF-010 | open | confirmed: first sentence at 889 px (Observe) and 881 px (Environment and admission) of 844 |
| UXF-011 | closed | holds as far as axe sees (0 nodes); type sizes not measured again |
| UXF-012 | closed | holds |
| UXF-013 | closed | holds: "run --observe" lands the row at 177 px |
| UXF-014 | open | confirmed: search "topology" ranks the fixture UI first, the CLI row third |
| UXF-015 | open | unchanged: http-call 3 actions at 1280x800, 5 at 390x844 |
| UXF-016 | closed | holds: 1 to 6 of 6 |
| UXF-017 | closed | holds for the landing (row at 177 px, words marked); the hit ranks second under another sentence's snippet: UXF-024 |
| UXF-018 | closed | holds: "version" answers 0.10.0 |
| UXF-019 | closed | holds: gate-1 0 at 1280x800; the MEANING column now carries the code blocks of UXF-023 |
| UXF-020 | open | confirmed: request-log-size 390x844 keyboard, both themes |
| UXF-021 | closed | not observed: the release keyboard stops above were not reproduced by hand, and no probe saw focus pulled to the field |

Regressed: request-log-size, reached in R-02 after, is not reached in R-03 (UXF-023, UXF-024).

## Findings filed

| id | title | severity | class | sessions | cost |
|---|---|---|---|---|---|
| UXF-023 | Code with a slash in a table cell renders as a code block; the --observe row grows to 1,474 px and pushes the request-log row two screens below where "run --observe" lands | S2 | layout | request-log-size, 8 plays (not reached); 41 extra Tab stops on 5 pages for every keyboard play there | S |
| UXF-024 | Searching "request records" ranks the Observe request-log section first; it neither gives the log's size nor links the --observe-requests row | S2 | cue | request-log-size, 8 plays (step 6); probe at all 4 viewport and theme pairs | S |
| UXF-025 | At desktop width the "On this page" list comes after the whole article in Tab order; Getting started's Build link takes 75 Tabs | S3 | input | install 1280x800 keyboard, 2 plays (86 Tabs) | M |

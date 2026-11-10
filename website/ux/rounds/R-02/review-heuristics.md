# R-02 heuristic review

Reviewer 1 (heuristics), fresh agent, 2026-11-10. Inputs: `report.md`, `shots/`, the four contact
sheets, the run snapshot `.out/R-02-run/` (pages, plays, verdicts, shots), `scenarios/cards.json`
and `tasks.json`, the operator notes, findings UXF-001..016. No recordings exist: evidence is a play
file, a screenshot or a probe of the frozen build `.out/R-02-build` (scripts and data in
`.out/review-1-R-02/`, shots `shots/review-*.png`).

**Verdict: 5 pass, 9 partial, 2 fail**

| # | heuristic | verdict | evidence |
|---|---|---|---|
| 1 | One next thing | pass | Landing names one primary action, "Get started" (contact sheets, landing frame); install reached in 1 action at 1280x800 (`plays/install--1280x800--light--pointer.json`); every docs page ends on one Next link. |
| 2 | State in words | pass | Status says "shipped"/"planned" in words in each chip and defines both above the table (`.out/R-02-run/shots/status--1280x800--light.png`); kickers say "Concepts · 3 of 6". |
| 3 | Nothing required hidden | fail | Phone: the answer column of every reference table is off screen; the `--observe-requests` row shows the flag, "no" and half of "256", its meaning at x=431 in a 390 px view (`shots/review-cli-observe-requests-row--390x844--light.png`); observe, topology, request-log-size not reached at 390x844 (12 plays). UXF-009 confirmed. Desktop: the run options table cuts its MEANING column by 133 px at 1280x800 (`shots/review-row-landing--1280x800--light.png`), new UXF-019. |
| 4 | Stable layout | partial | layout_shift 0 on 12 of 13 pages, Status 0.005 (`pages/status--1280x800--light.json`). Opening the search on a phone moves the menu and theme buttons 95 to 100 px left, cuts "Spindle" to "Spi…" and clips the typed query under the clear button (`shots/review-search-open-navbar--390x844--light.png`). Not filed: the reader's attention is in the field; no play lost time to it. |
| 5 | Provenance of every value | partial | Each docs header lists its source files, the CLI reference says "Generated from crates/spindle-cli/src/cli.rs", Getting started dates its pasted output (contact sheets). The search shows that output's "b10x-spindle 0.6.0" as the first answer to "version" without the date, three releases behind the changelog: new UXF-018. |
| 6 | Errors cheap | pass | Every wrong turn in a reached play is left with one link (`plays/http--1280x800--light--pointer.json`, 1 detour, 2 actions); a wrong query is cleared with one control (✕ in the field, `shots/review-search-landing--1280x800--light.png`). |
| 7 | Words the user can say | partial | Navigation labels read out unchanged ("Serving ingress.network over HTTP", "run --observe"). Search results start mid-sentence with punctuation, "…), refuses only the request that selects it: its …" (`shots/review-search-request-records--1280x800--light.png`); part of UXF-017. |
| 8 | The reader's time respected | partial | Release still 17 actions at 1280x800 and 21 at 390x844; search "changelog" finds nothing, "release" offers the `cargo build --release` line (`plays/release--1280x800--light--pointer.json`): UXF-006 confirmed. Phone headers push the first sentence to about 810 of 844 px on Observe (`.out/R-02-run/shots/concepts-observe--390x844--light.png`): UXF-010 confirmed. |
| 9 | Keyboard and pointer parity | pass | Every task the pointer reaches the keyboard reaches (report table). The search field takes focus after 10 Tabs at 1280 and 5 at 390, with a 2 px solid focus ring and a skip link first (probe `tabs-*`, `shots/review-search-focused--390x844--light.png`). Caveat: the 390x844 keyboard plays never use the search (pointer plays do), so their numbers (http-call 24 actions and 72 Tabs against 5 pointer actions) overstate the keyboard cost; a play tool matter, not the UI. |
| 10 | Recognition over recall | partial | Search "topology" puts Observe's fixture-UI sentence first and the CLI run section third (probe): UXF-014 confirmed. http-call reached in 3 actions at 1280x800 by search, 5 at 390x844 (`plays/http-call--390x844--light--pointer.json`): UXF-015 improved, its 3-action checklist still fails on the phone. A reader with the answer's words ("request records") is not led to the row: UXF-017. |
| 11 | Calm by default | pass | No banner, toast or motion on any frame of the four contact sheets; `scroll-behavior: auto`; layout_shift as in 4. |
| 12 | One state vocabulary | partial | Overview "1 of 6" and Getting started "2 of 6" share the wording of "Concepts · 1 of 6" (`shots/review-search-request-records--1280x800--light.png`, contact sheets): UXF-012 confirmed. Concept kickers read 1 to 6 of 6 without gaps: UXF-016's fix holds. |
| 13 | A wrong cue recovered in one action | fail | Search "request records" gives one hit; it opens the run heading with the answer row 5.8 screens below at 1280x800 and 7.8 at 390x844, and nothing on the landing says the answer is below (`shots/review-search-landing--1280x800--light.png`, `--390x844--light.png`): new UXF-017. request-log-size 390x844: search, Escape, 15 scrolls, not reached (`plays/request-log-size--390x844--light--pointer.json`). |
| 14 | Unmistakable end state | partial | The "run --observe" sidebar link and a row URL land with the row 177 px from the top: UXF-013's fix holds. The target row carries no mark (`:target` background transparent), and the search ends on a heading (UXF-017). Unmarked row not filed: it is the first row under the navbar in both shots. |
| 15 | Readable in every theme, key data at 14 px | partial | Table cells, code blocks and status chip text are 14 px in both themes (probe `static-*`, `probe3.json`): UXF-011's fix holds; axe 0 serious or critical nodes on 52 page checks. The light navbar's "Search" placeholder is rgb(190,195,201) on rgb(214,223,218), 1.3:1 (5.9:1 in dark) (`.out/R-02-run/shots/search--1280x800--light.png`). Not filed: the magnifier and "ctrl K" mark the field. |
| 16 | Focus follows the work | partial | A sidebar link to a row puts the row in view (UXF-013 holds); a search hit inside a table puts the section heading in view, 6 to 8 screens from the row (UXF-017). |

## Existing findings

| id | status | in R-02 |
|---|---|---|
| UXF-001 | closed | holds: the search field is in the navbar at every gating viewport and theme (contact sheets) |
| UXF-002 | closed | holds: theme toggle at 274..306 px of 390 (probe `static-390x844-*`) |
| UXF-003 | closed | holds: prose links underlined (probe `static-*`), axe 0 |
| UXF-004 | closed | holds: http reached in 2 actions at 1280x800, 3 at 390x844 |
| UXF-005 | closed | holds at 1280x800 (observe 1 action); 390x844 fails under UXF-009 |
| UXF-006 | open | confirmed: 17 and 21 actions; search does not reach the changelog |
| UXF-007 | closed | holds for the sentence; the pasted 0.6.0 output it noted is now UXF-018 |
| UXF-008 | open | confirmed: gate-2 end-state miss on every docs page at 1280x800 (22 known-failing) |
| UXF-009 | open | confirmed and wider: request-log-size at 390x844 also fails on it; its 4 red verdicts (gate-4 and gate-5-keyboard, both themes) carry no finding and no known-failing mark |
| UXF-010 | open | confirmed: Observe's first sentence at about 810 of 844 px |
| UXF-011 | closed | holds: key data 14 px; side effect: the run options table overflows more at 1280 (UXF-019) |
| UXF-012 | open | confirmed |
| UXF-013 | closed | holds: the row anchor lands on the row |
| UXF-014 | open | confirmed: search "topology" ranks the fixture UI first |
| UXF-015 | open | improved by the search (3 actions at 1280x800); 5 at 390x844, checklist still fails |
| UXF-016 | closed | holds: 1 to 6 of 6 |

None regressed.

## Findings filed

| id | title | severity | class | sessions | cost |
|---|---|---|---|---|---|
| UXF-017 | A search hit in a reference table opens the section heading, 6 to 8 screens above the row, under a snippet of another sentence | S2 | cue | request-log-size 390x844, 4 plays (not reached); probe at all 4 viewport and theme pairs | M |
| UXF-018 | Searching "version" answers b10x-spindle 0.6.0; the changelog's latest release is 0.9.0 | S3 | copy | no play searched "version"; probe at all 4 pairs | S |
| UXF-019 | At 1280 px the run options table cuts its MEANING column by 133 px, where three tasks' answers are | S3 | layout | reference-cli 1280x800, 4 page checks; observe, topology, request-log-size 1280x800, 12 plays | S |

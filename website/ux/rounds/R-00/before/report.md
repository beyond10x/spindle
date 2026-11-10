# R-00 before round report

Date 2026-11-10, commit `244fb60e+uncommitted`. Spindle documentation site (website/, Docusaurus). Hermetic: the built site served from `website/build` on 127.0.0.1, every request off the site answered by a stub page; no keys.
Page checks: 48 (12 sitemap pages × 2 gating viewports × 2 themes), measured-only: 72. Screen-only plays: 40 (5 tasks × 2 viewports × 2 themes × pointer and keyboard).

## Hard gates

Verdicts over every gating check, judged again against `ux/known-failing.json` as it stands; known-failing = covered by a mark with its open finding. As run, 0 of 184 verdicts were red (against the marks of that moment). Site measured: website/build.

| gate | what | pass | fail | known-failing | findings |
|---|---|---|---|---|---|
| gate-1 | no horizontal scroll, nothing past its box | 48 | 0 | 0 | – |
| gate-2 | always visible | 0 | 0 | 48 | UXF-001, UXF-002, UXF-008 |
| gate-4 | screen-only reader reaches the truth | 12 | 0 | 8 | UXF-004, UXF-005 |
| gate-5 | accessibility (axe) | 20 | 0 | 28 | UXF-003 |
| gate-5-keyboard | accessibility (keyboard only) | 12 | 0 | 8 | UXF-004, UXF-005 |
| gate-3 | unmissable completion | – | – | – | docs: the truth in view at the end of gate 4 |
| gate-6 | cue truth | – | – | – | dropped: no guidance cue |

## Per page, viewport and theme

Elements past their box (worst state), always-visible misses (top + end), axe serious/critical nodes.

| page | 1280x800 light | 1280x800 dark | 390x844 light | 390x844 dark |
|---|---|---|---|---|
| landing | 0 / 2 / 0 | 0 / 2 / 0 | 0 / 4 / 0 | 0 / 4 / 0 |
| docs | 0 / 3 / 8 | 0 / 3 / 8 | 0 / 4 / 8 | 0 / 4 / 8 |
| concepts-capabilities | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 4 / 0 | 0 / 4 / 0 |
| concepts-components-and-bindings | 0 / 3 / 1 | 0 / 3 / 1 | 0 / 4 / 1 | 0 / 4 / 1 |
| concepts-deployment-targets | 0 / 3 / 1 | 0 / 3 / 1 | 0 / 4 / 1 | 0 / 4 / 1 |
| concepts-environment-and-admission | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 4 / 0 | 0 / 4 / 0 |
| concepts-observe | 0 / 3 / 1 | 0 / 3 / 1 | 0 / 4 / 1 | 0 / 4 / 1 |
| concepts-specification | 0 / 3 / 3 | 0 / 3 / 3 | 0 / 4 / 3 | 0 / 4 / 3 |
| examples-rally | 0 / 3 / 2 | 0 / 3 / 2 | 0 / 4 / 2 | 0 / 4 / 2 |
| getting-started | 0 / 3 / 7 | 0 / 3 / 7 | 0 / 4 / 7 | 0 / 4 / 7 |
| reference-cli (generated) | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 4 / 0 | 0 / 4 / 0 |
| status (generated) | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 4 / 0 | 0 / 4 / 0 |

## Screen-only reader per task

Pointer plays; `tabs` from the keyboard play at the same viewport and theme.

| task | card | viewport | theme | reached | actions | looks | detours | keyboard reached | tabs | path |
|---|---|---|---|---|---|---|---|---|---|---|
| install | A-newcomer (anchor) | 1280x800 | dark | true | 1 | 2 | 0 | true | 10 | link "Get started →" |
| install | A-newcomer (anchor) | 1280x800 | light | true | 1 | 2 | 0 | true | 10 | link "Get started →" |
| install | A-newcomer (anchor) | 390x844 | dark | true | 5 | 7 | 0 | true | 13 | link "Get started →" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "Build" |
| install | A-newcomer (anchor) | 390x844 | light | true | 5 | 7 | 0 | true | 13 | link "Get started →" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "Build" |
| http | B-integrator (anchor) | 1280x800 | dark | false | 17 | 48 | 2 | false | 13 | link "Get started →" → scroll ×12 → link "Documentation" (explore) → scroll ×3 — stuck: nothing on screen carries "HTTP", "serve", "ingress", "run" at the end of /spindle/docs |
| http | B-integrator (anchor) | 1280x800 | light | false | 17 | 48 | 2 | false | 13 | link "Get started →" → scroll ×12 → link "Documentation" (explore) → scroll ×3 — stuck: nothing on screen carries "HTTP", "serve", "ingress", "run" at the end of /spindle/docs |
| http | B-integrator (anchor) | 390x844 | dark | false | 19 | 47 | 1 | false | 98 | link "Get started →" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → scroll ×13 → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) — stuck: nothing on screen carries "HTTP", "serve", "ingress", "run" at the end of /spindle/docs/getting-started |
| http | B-integrator (anchor) | 390x844 | light | false | 19 | 47 | 1 | false | 98 | link "Get started →" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → scroll ×13 → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) — stuck: nothing on screen carries "HTTP", "serve", "ingress", "run" at the end of /spindle/docs/getting-started |
| observe | R00-operator | 1280x800 | dark | false | 9 | 20 | 3 | false | 95 | link "Observe" → link "The observe API and UI" → scroll → link "docs/design/observe.md" → scroll → link "Documentation" (explore) → scroll ×3 — stuck: nothing on screen carries "--observe", "observe", "run" at the end of /spindle/docs |
| observe | R00-operator | 1280x800 | light | false | 9 | 20 | 3 | false | 95 | link "Observe" → link "The observe API and UI" → scroll → link "docs/design/observe.md" → scroll → link "Documentation" (explore) → scroll ×3 — stuck: nothing on screen carries "--observe", "observe", "run" at the end of /spindle/docs |
| observe | R00-operator | 390x844 | dark | false | 12 | 20 | 2 | false | 61 | button "Toggle navigation bar" (reveal) → link "Observe" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "The observe API and UI" → scroll ×2 → link "docs/design/observe.md" → scroll → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) — stuck: nothing on screen carries "--observe", "observe", "run" at the end of /spindle/docs/concepts/observe |
| observe | R00-operator | 390x844 | light | false | 12 | 20 | 2 | false | 61 | button "Toggle navigation bar" (reveal) → link "Observe" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "The observe API and UI" → scroll ×2 → link "docs/design/observe.md" → scroll → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) — stuck: nothing on screen carries "--observe", "observe", "run" at the end of /spindle/docs/concepts/observe |
| release | A-newcomer (anchor) | 1280x800 | dark | true | 17 | 48 | 1 | true | 132 | link "Status" → scroll ×15 → link "Changelog" |
| release | A-newcomer (anchor) | 1280x800 | light | true | 17 | 48 | 1 | true | 131 | link "Status" → scroll ×15 → link "Changelog" |
| release | A-newcomer (anchor) | 390x844 | dark | true | 26 | 68 | 1 | true | 120 | button "Toggle navigation bar" (reveal) → link "Status" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → scroll ×20 → link "Changelog" |
| release | A-newcomer (anchor) | 390x844 | light | true | 26 | 68 | 1 | true | 120 | button "Toggle navigation bar" (reveal) → link "Status" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → scroll ×20 → link "Changelog" |
| environment | B-integrator (anchor) | 1280x800 | dark | true | 3 | 8 | 0 | true | 14 | scroll ×2 → link "Admission or a named refusal" |
| environment | B-integrator (anchor) | 1280x800 | light | true | 3 | 8 | 0 | true | 14 | scroll ×2 → link "Admission or a named refusal" |
| environment | B-integrator (anchor) | 390x844 | dark | true | 6 | 13 | 0 | true | 11 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → scroll ×3 → link "Admission or a named refusal" |
| environment | B-integrator (anchor) | 390x844 | light | true | 6 | 13 | 0 | true | 11 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → scroll ×3 → link "Admission or a named refusal" |

Truth per task:

- `install` "Find the install command": `/docs/getting-started` #build, text "cargo build --locked --release -p spindle-cli"; words "install", "get started", "getting started", "build"; start `/`.
- `http` "Find how to run a specification over HTTP": `/docs/concepts/capabilities` #serving-ingressnetwork-over-http, text "over HTTP on"; words "HTTP", "serve", "ingress", "run"; start `/`.
- `observe` "Find what run --observe serves": `/docs/reference/cli` #run, text "Record every request the listener answers outside"; words "--observe", "observe", "run"; start `/docs/examples/rally`.
- `release` "Find the release and version status": off the site, https://github.com/beyond10x/spindle/blob/main/CHANGELOG.md; words "release", "version", "changelog", "status"; start `/`.
- `environment` "Find what an environment document is": `/docs/concepts/environment-and-admission` #the-environment, text "An environment document"; words "environment document", "environment", "admission"; start `/`.

## Metrics against the bars

Every key lower-is-better; `base` is `ux/baseline.json`, `last` the previous round. Task keys: worst over the gating viewports and themes.

| scenario | screen_only.actions | screen_only.looks | screen_only.detours | glance_cost.median | glance_cost.p95 | keyboard.tabs |
|---|---|---|---|---|---|---|
| install | 5 (base 5, last –) | 7 (base 7, last –) | 0 (base 0, last –) | 2 (base 2, last –) | 2 (base 2, last –) | 13 (base 13, last –) |
| http | 31 (base 31, last –) | 93 (base 93, last –) | 2 (base 2, last –) | 3 (base 3, last –) | 3 (base 3, last –) | 400 (base 400, last –) |
| observe | 31 (base 31, last –) | 93 (base 93, last –) | 3 (base 3, last –) | 3 (base 3, last –) | 3 (base 3, last –) | 400 (base 400, last –) |
| release | 26 (base 26, last –) | 68 (base 68, last –) | 1 (base 1, last –) | 3 (base 3, last –) | 3 (base 3, last –) | 132 (base 132, last –) |
| environment | 6 (base 6, last –) | 13 (base 13, last –) | 0 (base 0, last –) | 3 (base 3, last –) | 3 (base 3, last –) | 14 (base 14, last –) |

| page | axe.light | axe.dark | overflow.elements | always_visible.misses | reading_load.words | reading_load.long_paragraphs | reading_load.max_paragraph |
|---|---|---|---|---|---|---|---|
| landing | 0 (base 0) | 0 (base 0) | 0 (base 0) | 6 (base 6) | 2249 (base 2249) | 0 (base 0) | 51 (base 51) |
| docs | 8 (base 8) | 8 (base 8) | 0 (base 0) | 7 (base 7) | 459 (base 459) | 1 (base 1) | 84 (base 84) |
| concepts-capabilities | 0 (base 0) | 0 (base 0) | 0 (base 0) | 7 (base 7) | 1049 (base 1049) | 2 (base 2) | 213 (base 213) |
| concepts-components-and-bindings | 1 (base 1) | 1 (base 1) | 0 (base 0) | 7 (base 7) | 506 (base 506) | 1 (base 1) | 83 (base 83) |
| concepts-deployment-targets | 1 (base 1) | 1 (base 1) | 0 (base 0) | 7 (base 7) | 485 (base 485) | 0 (base 0) | 59 (base 59) |
| concepts-environment-and-admission | 0 (base 0) | 0 (base 0) | 0 (base 0) | 7 (base 7) | 511 (base 511) | 0 (base 0) | 68 (base 68) |
| concepts-observe | 1 (base 1) | 1 (base 1) | 0 (base 0) | 7 (base 7) | 936 (base 936) | 2 (base 2) | 155 (base 155) |
| concepts-specification | 3 (base 3) | 3 (base 3) | 0 (base 0) | 7 (base 7) | 987 (base 987) | 4 (base 4) | 181 (base 181) |
| examples-rally | 2 (base 2) | 2 (base 2) | 0 (base 0) | 7 (base 7) | 1025 (base 1025) | 2 (base 2) | 194 (base 194) |
| getting-started | 7 (base 7) | 7 (base 7) | 0 (base 0) | 7 (base 7) | 1233 (base 1233) | 0 (base 0) | 74 (base 74) |
| reference-cli | 0 (base 0) | 0 (base 0) | 0 (base 0) | 7 (base 7) | 3358 (base 3358) | 11 (base 11) | 322 (base 322) |
| status | 0 (base 0) | 0 (base 0) | 0 (base 0) | 7 (base 7) | 2791 (base 2791) | 0 (base 0) | 51 (base 51) |

Reading load: median 1006 words per page, 23 paragraphs over 80 words in all.

## Worst pages

| viewport | theme | page | gate failures | shot |
|---|---|---|---|---|
| 1280x800 | light | getting-started | 4 | [worst--getting-started--1280x800--light.png](shots/worst--getting-started--1280x800--light.png) |
| 1280x800 | dark | getting-started | 4 | [worst--getting-started--1280x800--dark.png](shots/worst--getting-started--1280x800--dark.png) |
| 390x844 | light | getting-started | 5 | [worst--getting-started--390x844--light.png](shots/worst--getting-started--390x844--light.png) |
| 390x844 | dark | getting-started | 5 | [worst--getting-started--390x844--dark.png](shots/worst--getting-started--390x844--dark.png) |

## Contact sheets

- [shots/contact--1280x800--light.jpg](shots/contact--1280x800--light.jpg): every page at the top, after load and settle.
- [shots/contact--1280x800--dark.jpg](shots/contact--1280x800--dark.jpg): every page at the top, after load and settle.
- [shots/contact--390x844--light.jpg](shots/contact--390x844--light.jpg): every page at the top, after load and settle.
- [shots/contact--390x844--dark.jpg](shots/contact--390x844--dark.jpg): every page at the top, after load and settle.

## Measured-only viewports

Reported, not gated: elements past their box, always-visible misses and axe serious/critical nodes, worst over the themes.

| page | 1440x900 | 1024x768 | 768x1024 |
|---|---|---|---|
| landing | 0 / 2 / 0 | 0 / 2 / 0 | 0 / 4 / 0 |
| docs | 0 / 3 / 8 | 0 / 3 / 8 | 0 / 4 / 8 |
| concepts-capabilities | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 4 / 0 |
| concepts-components-and-bindings | 0 / 3 / 1 | 0 / 3 / 1 | 0 / 4 / 1 |
| concepts-deployment-targets | 0 / 3 / 1 | 0 / 3 / 1 | 0 / 4 / 1 |
| concepts-environment-and-admission | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 4 / 0 |
| concepts-observe | 0 / 3 / 1 | 0 / 3 / 1 | 0 / 4 / 1 |
| concepts-specification | 0 / 3 / 3 | 0 / 3 / 3 | 0 / 4 / 3 |
| examples-rally | 0 / 3 / 2 | 0 / 3 / 2 | 0 / 4 / 2 |
| getting-started | 0 / 3 / 7 | 0 / 3 / 7 | 0 / 4 / 7 |
| reference-cli | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 4 / 0 |
| status | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 4 / 0 |

## Not measured in this round

- Questionnaire scores: none, no people (L6 not run); the cards were played by the screen-only reader.
- L5 discoverability probe misses: not run (paid, operator-only). L4 keyed replays: not applicable (a static site).
- L0 and gate 6: dropped, no guidance cue. Gate 3 is the truth in view at the end of gate 4.
- Recordings (rrweb): none; the evidence is the page checks, the plays and the shots.

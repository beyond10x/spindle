# R-01 round report

Date 2026-11-10, commit `82faeb87+uncommitted`. Spindle documentation site (website/, Docusaurus). Hermetic: the built site served from `website/build` on 127.0.0.1, every request off the site answered by a stub page; no keys.
Page checks: 48 (12 sitemap pages × 2 gating viewports × 2 themes), measured-only: 72. Screen-only plays: 56 (7 tasks × 2 viewports × 2 themes × pointer and keyboard).

## Hard gates

Verdicts over every gating check, judged again against `ux/known-failing.json` as it stands; known-failing = covered by a mark with its open finding. As run, 8 of 200 verdicts were red (against the marks of that moment). Site measured: website/build.

| gate | what | pass | fail | known-failing | findings |
|---|---|---|---|---|---|
| gate-1 | no horizontal scroll, nothing past its box | 48 | 0 | 0 | – |
| gate-2 | always visible | 0 | 0 | 48 | UXF-001, UXF-008 |
| gate-4 | screen-only reader reaches the truth | 22 | 4 | 2 | UXF-009 |
| gate-5 | accessibility (axe) | 48 | 0 | 0 | – |
| gate-5-keyboard | accessibility (keyboard only) | 22 | 4 | 2 | UXF-009 |
| gate-3 | unmissable completion | – | – | – | docs: the truth in view at the end of gate 4 |
| gate-6 | cue truth | – | – | – | dropped: no guidance cue |

## Per page, viewport and theme

Elements past their box (worst state), always-visible misses (top + end), axe serious/critical nodes.

| page | 1280x800 light | 1280x800 dark | 390x844 light | 390x844 dark |
|---|---|---|---|---|
| landing | 0 / 2 / 0 | 0 / 2 / 0 | 0 / 2 / 0 | 0 / 2 / 0 |
| docs | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 | 0 / 2 / 0 |
| concepts-capabilities | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 | 0 / 2 / 0 |
| concepts-components-and-bindings | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 | 0 / 2 / 0 |
| concepts-deployment-targets | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 | 0 / 2 / 0 |
| concepts-environment-and-admission | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 | 0 / 2 / 0 |
| concepts-observe | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 | 0 / 2 / 0 |
| concepts-specification | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 | 0 / 2 / 0 |
| examples-rally | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 | 0 / 2 / 0 |
| getting-started | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 | 0 / 2 / 0 |
| reference-cli (generated) | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 | 0 / 2 / 0 |
| status (generated) | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 | 0 / 2 / 0 |

## Screen-only reader per task

Pointer plays; `tabs` from the keyboard play at the same viewport and theme.

| task | card | viewport | theme | reached | actions | looks | detours | keyboard reached | tabs | path |
|---|---|---|---|---|---|---|---|---|---|---|
| install | A-newcomer (anchor) | 1280x800 | dark | true | 1 | 2 | 0 | true | 10 | link "Get started →" |
| install | A-newcomer (anchor) | 1280x800 | light | true | 1 | 2 | 0 | true | 10 | link "Get started →" |
| install | A-newcomer (anchor) | 390x844 | dark | true | 5 | 7 | 0 | true | 15 | link "Get started →" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "Build" |
| install | A-newcomer (anchor) | 390x844 | light | true | 5 | 7 | 0 | true | 15 | link "Get started →" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "Build" |
| http | B-integrator (anchor) | 1280x800 | dark | true | 2 | 3 | 1 | true | 25 | link "Get started →" → link "Serving ingress.network over HTTP" |
| http | B-integrator (anchor) | 1280x800 | light | true | 2 | 3 | 1 | true | 35 | link "Get started →" → link "Serving ingress.network over HTTP" |
| http | B-integrator (anchor) | 390x844 | dark | true | 3 | 4 | 1 | true | 19 | link "Get started →" → button "Toggle navigation bar" (reveal) → link "Serving ingress.network over HTTP" |
| http | B-integrator (anchor) | 390x844 | light | true | 3 | 4 | 1 | true | 19 | link "Get started →" → button "Toggle navigation bar" (reveal) → link "Serving ingress.network over HTTP" |
| observe | R00-operator | 1280x800 | dark | true | 7 | 19 | 0 | true | 20 | link "run --observe" → read-scroll ×6 |
| observe | R00-operator | 1280x800 | light | true | 7 | 19 | 0 | true | 20 | link "run --observe" → read-scroll ×6 |
| observe | R00-operator | 390x844 | dark | false | 30 | 78 | 1 | false | 62 | button "Toggle navigation bar" (reveal) → link "run --observe" → read-scroll ×21 → link "Observe" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "The observe API and UI" → scroll ×2 — stuck: not there after 30 actions |
| observe | R00-operator | 390x844 | light | false | 30 | 78 | 1 | false | 62 | button "Toggle navigation bar" (reveal) → link "run --observe" → read-scroll ×21 → link "Observe" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "The observe API and UI" → scroll ×2 — stuck: not there after 30 actions |
| release | A-newcomer (anchor) | 1280x800 | dark | true | 17 | 48 | 1 | true | 134 | link "Status" → scroll ×15 → link "Changelog" |
| release | A-newcomer (anchor) | 1280x800 | light | true | 17 | 48 | 1 | true | 133 | link "Status" → scroll ×15 → link "Changelog" |
| release | A-newcomer (anchor) | 390x844 | dark | true | 26 | 68 | 1 | true | 122 | button "Toggle navigation bar" (reveal) → link "Status" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → scroll ×20 → link "Changelog" |
| release | A-newcomer (anchor) | 390x844 | light | true | 26 | 68 | 1 | true | 122 | button "Toggle navigation bar" (reveal) → link "Status" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → scroll ×20 → link "Changelog" |
| environment | B-integrator (anchor) | 1280x800 | dark | true | 3 | 8 | 0 | true | 14 | scroll ×2 → link "Admission or a named refusal" |
| environment | B-integrator (anchor) | 1280x800 | light | true | 3 | 8 | 0 | true | 14 | scroll ×2 → link "Admission or a named refusal" |
| environment | B-integrator (anchor) | 390x844 | dark | true | 6 | 13 | 0 | true | 12 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → scroll ×3 → link "Admission or a named refusal" |
| environment | B-integrator (anchor) | 390x844 | light | true | 6 | 13 | 0 | true | 12 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → scroll ×3 → link "Admission or a named refusal" |
| http-call | R01-caller | 1280x800 | dark | true | 10 | 21 | 1 | true | 181 | scroll ×5 → link "External decisions from a file" → link "Answering external outcomes" → link "Serving ingress.network over HTTP" → link "Environment and admission" → link "An external cause bound to an HTTP call" |
| http-call | R01-caller | 1280x800 | light | true | 10 | 21 | 1 | true | 181 | scroll ×5 → link "External decisions from a file" → link "Answering external outcomes" → link "Serving ingress.network over HTTP" → link "Environment and admission" → link "An external cause bound to an HTTP call" |
| http-call | R01-caller | 390x844 | dark | true | 24 | 53 | 1 | true | 69 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → scroll ×9 → link "External decisions from a file" → button "Toggle navigation bar" (reveal) → link "Serving ingress.network over HTTP" → scroll → button "Toggle word wrap" → button "Copy code to clipboard" → scroll ×2 → link "NEXT Environment and admission" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "An external cause bound to an HTTP call" |
| http-call | R01-caller | 390x844 | light | true | 24 | 53 | 1 | true | 69 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → scroll ×9 → link "External decisions from a file" → button "Toggle navigation bar" (reveal) → link "Serving ingress.network over HTTP" → scroll → button "Toggle word wrap" → button "Copy code to clipboard" → scroll ×2 → link "NEXT Environment and admission" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "An external cause bound to an HTTP call" |
| topology | R01-caller | 1280x800 | dark | false | 10 | 21 | 2 | false | 186 | link "run --observe" → link "Observe" → link "The observe API and UI" → scroll → link "docs/design/observe.md" → scroll → link "Documentation" (explore) → scroll ×3 — stuck: nothing on screen carries "topology", "--observe", "observe" at the end of /spindle/docs |
| topology | R01-caller | 1280x800 | light | false | 10 | 21 | 2 | false | 185 | link "run --observe" → link "Observe" → link "The observe API and UI" → scroll → link "docs/design/observe.md" → scroll → link "Documentation" (explore) → scroll ×3 — stuck: nothing on screen carries "topology", "--observe", "observe" at the end of /spindle/docs |
| topology | R01-caller | 390x844 | dark | false | 12 | 20 | 1 | false | 120 | button "Toggle navigation bar" (reveal) → link "run --observe" → button "Toggle navigation bar" (reveal) → link "Observe" → button "On this page" (reveal) → link "The observe API and UI" → scroll ×2 → link "docs/design/observe.md" → scroll → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) — stuck: nothing on screen carries "topology", "--observe", "observe" at the end of /spindle/docs/concepts/observe |
| topology | R01-caller | 390x844 | light | false | 12 | 20 | 1 | false | 120 | button "Toggle navigation bar" (reveal) → link "run --observe" → button "Toggle navigation bar" (reveal) → link "Observe" → button "On this page" (reveal) → link "The observe API and UI" → scroll ×2 → link "docs/design/observe.md" → scroll → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) — stuck: nothing on screen carries "topology", "--observe", "observe" at the end of /spindle/docs/concepts/observe |

Truth per task:

- `install` "Find the install command": `/docs/getting-started` #build, text "cargo build --locked --release -p spindle-cli"; words "install", "get started", "getting started", "build"; start `/`.
- `http` "Find how to run a specification over HTTP": `/docs/concepts/capabilities` #serving-ingressnetwork-over-http, text "over HTTP on"; words "HTTP", "serve", "ingress", "run"; start `/`.
- `observe` "Find what run --observe serves": `/docs/reference/cli` #run, text "Record every request the listener answers outside"; words "--observe", "observe", "run"; start `/docs/examples/rally`.
- `release` "Find the release and version status": off the site, https://github.com/beyond10x/spindle/blob/main/CHANGELOG.md; words "release", "version", "changelog", "status"; start `/`.
- `environment` "Find what an environment document is": `/docs/concepts/environment-and-admission` #the-environment, text "An environment document"; words "environment document", "environment", "admission"; start `/`.
- `http-call` "Find how to answer an external cause with an outbound HTTP call": `/docs/concepts/environment-and-admission` #an-external-cause-bound-to-an-http-call, text "An environment may declare HTTP calls"; words "HTTP call", "external", "HTTP", "environment"; start `/`.
- `topology` "Find what the topology of an observed run answers": `/docs/reference/cli` #run, text "graph derived from the specification"; words "topology", "--observe", "observe"; start `/docs/concepts/observe`.

## Metrics against the bars

Every key lower-is-better; `base` is `ux/baseline.json`, `last` the previous round. Task keys: worst over the gating viewports and themes.

| scenario | screen_only.actions | screen_only.looks | screen_only.detours | glance_cost.median | glance_cost.p95 | keyboard.tabs |
|---|---|---|---|---|---|---|
| install | 5 (base 5, last 5) | 7 (base 7, last 7) | 0 (base 0, last 0) | 2 (base 2, last 2) | 2 (base 2, last 2) | 15 (base 15, last 15) |
| http | 3 (base 3, last 3) | 4 (base 4, last 4) | 1 (base 1, last 1) | 1.5 (base 1.5, last 1.5) | 2 (base 2, last 2) | 35 (base 25, last 25) |
| observe | 31 (base 31, last 31) | 93 (base 93, last 93) | 1 (base 2, last 2) | 3 (base 3, last 3) | 3 (base 3, last 3) | 400 (base 400, last 400) |
| release | 26 (base 26, last 26) | 68 (base 68, last 68) | 1 (base 1, last 1) | 3 (base 3, last 3) | 3 (base 3, last 3) | 134 (base 134, last 134) |
| environment | 6 (base 6, last 6) | 13 (base 13, last 13) | 0 (base 0, last 0) | 3 (base 3, last 3) | 3 (base 3, last 3) | 14 (base 14, last 14) |
| http-call | 24 (base –, last –) | 53 (base –, last –) | 1 (base –, last –) | 2.5 (base –, last –) | 3 (base –, last –) | 181 (base –, last –) |
| topology | 31 (base –, last –) | 93 (base –, last –) | 2 (base –, last –) | 3 (base –, last –) | 3 (base –, last –) | 400 (base –, last –) |

| page | axe.light | axe.dark | overflow.elements | always_visible.misses | layout_shift.score | reading_load.words | reading_load.long_paragraphs | reading_load.max_paragraph |
|---|---|---|---|---|---|---|---|---|
| landing | 0 (base 0) | 0 (base 0) | 0 (base 0) | 4 (base 4) | 0 (base –) | 2249 (base 2249) | 0 (base 0) | 51 (base 51) |
| docs | 0 (base 0) | 0 (base 0) | 0 (base 0) | 5 (base 5) | 0 (base –) | 459 (base 459) | 1 (base 1) | 84 (base 84) |
| concepts-capabilities | 0 (base 0) | 0 (base 0) | 0 (base 0) | 5 (base 5) | 0 (base –) | 1049 (base 1049) | 2 (base 2) | 213 (base 213) |
| concepts-components-and-bindings | 0 (base 0) | 0 (base 0) | 0 (base 0) | 5 (base 5) | 0 (base –) | 506 (base 506) | 1 (base 1) | 83 (base 83) |
| concepts-deployment-targets | 0 (base 0) | 0 (base 0) | 0 (base 0) | 5 (base 5) | 0 (base –) | 485 (base 485) | 0 (base 0) | 59 (base 59) |
| concepts-environment-and-admission | 0 (base 0) | 0 (base 0) | 0 (base 0) | 5 (base 5) | 0 (base –) | 818 (base 511) | 2 (base 0) | 121 (base 68) |
| concepts-observe | 0 (base 0) | 0 (base 0) | 0 (base 0) | 5 (base 5) | 0 (base –) | 936 (base 936) | 2 (base 2) | 155 (base 155) |
| concepts-specification | 0 (base 0) | 0 (base 0) | 0 (base 0) | 5 (base 5) | 0 (base –) | 987 (base 987) | 4 (base 4) | 181 (base 181) |
| examples-rally | 0 (base 0) | 0 (base 0) | 0 (base 0) | 5 (base 5) | 0 (base –) | 1025 (base 1025) | 2 (base 2) | 194 (base 194) |
| getting-started | 0 (base 0) | 0 (base 0) | 0 (base 0) | 5 (base 5) | 0.001 (base –) | 1233 (base 1233) | 0 (base 0) | 74 (base 74) |
| reference-cli | 0 (base 0) | 0 (base 0) | 0 (base 0) | 5 (base 5) | 0 (base –) | 3549 (base 3358) | 12 (base 11) | 322 (base 322) |
| status | 0 (base 0) | 0 (base 0) | 0 (base 0) | 5 (base 5) | 0 (base –) | 2791 (base 2791) | 0 (base 0) | 51 (base 51) |

Reading load: median 1006 words per page, 26 paragraphs over 80 words in all.

## Worst pages

| viewport | theme | page | gate failures | shot |
|---|---|---|---|---|
| 1280x800 | light | reference-cli | 3 | [worst--reference-cli--1280x800--light.png](shots/worst--reference-cli--1280x800--light.png) |
| 1280x800 | dark | reference-cli | 3 | [worst--reference-cli--1280x800--dark.png](shots/worst--reference-cli--1280x800--dark.png) |
| 390x844 | light | reference-cli | 2 | [worst--reference-cli--390x844--light.png](shots/worst--reference-cli--390x844--light.png) |
| 390x844 | dark | reference-cli | 2 | [worst--reference-cli--390x844--dark.png](shots/worst--reference-cli--390x844--dark.png) |

## Contact sheets

- [shots/contact--1280x800--light.jpg](shots/contact--1280x800--light.jpg): every page at the top, after load and settle.
- [shots/contact--1280x800--dark.jpg](shots/contact--1280x800--dark.jpg): every page at the top, after load and settle.
- [shots/contact--390x844--light.jpg](shots/contact--390x844--light.jpg): every page at the top, after load and settle.
- [shots/contact--390x844--dark.jpg](shots/contact--390x844--dark.jpg): every page at the top, after load and settle.

## Measured-only viewports

Reported, not gated: elements past their box, always-visible misses and axe serious/critical nodes, worst over the themes.

| page | 1440x900 | 1024x768 | 768x1024 |
|---|---|---|---|
| landing | 0 / 2 / 0 | 0 / 2 / 0 | 0 / 2 / 0 |
| docs | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 |
| concepts-capabilities | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 |
| concepts-components-and-bindings | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 |
| concepts-deployment-targets | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 |
| concepts-environment-and-admission | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 |
| concepts-observe | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 |
| concepts-specification | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 |
| examples-rally | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 |
| getting-started | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 |
| reference-cli | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 |
| status | 0 / 3 / 0 | 0 / 3 / 0 | 0 / 2 / 0 |

## Not measured in this round

- Questionnaire scores: none, no people (L6 not run); the cards were played by the screen-only reader.
- L5 discoverability probe misses: not run (paid, operator-only). L4 keyed replays: not applicable (a static site).
- L0 and gate 6: dropped, no guidance cue. Gate 3 is the truth in view at the end of gate 4.
- Recordings (rrweb): none; the evidence is the page checks, the plays and the shots.

## Caveats (added after the review, `review-report.md`)

- As run, 8 verdicts were red: `topology` gate 4 and the keyboard gate in all 8 plays, with no mark and no finding. UXF-013 (filed by the heuristic review) covers them; the change set fixes them at 1280x800, and at 390x844 they are marked with UXF-009.
- The ratchet failed on this measurement: 5 reading-load keys worse from content main added (taken in under `R-01-base-82faeb87`), `http` `keyboard.tabs` 35 against 25 (a timing-dependent path, tool defect T5), and 24 keys without a baseline (12 `layout_shift.score`, 12 of the two new tasks).
- The trend sums rose from 71 to 126 actions only because two tasks were added; on the 5 R-00 tasks they are unchanged (71 actions, 185 looks).
- `observe` detours 2 → 1 is the action cap cutting the unreached 390x844 path one step earlier, not an improvement; it was not taken into the baseline.
- The site measured is frozen at `.out/R-01-build/` (git-ignored); `website/build` was rebuilt after the run.

# R-01 after round report

Date 2026-11-10, commit `82faeb87+uncommitted`. Spindle documentation site (website/, Docusaurus). Hermetic: the built site served from `website/build` on 127.0.0.1, every request off the site answered by a stub page; no keys.
Page checks: 52 (13 sitemap pages × 2 gating viewports × 2 themes), measured-only: 78. Screen-only plays: 56 (7 tasks × 2 viewports × 2 themes × pointer and keyboard).

## Hard gates

Verdicts over every gating check, judged again against `ux/known-failing.json` as it stands; known-failing = covered by a mark with its open finding. As run, 0 of 212 verdicts were red (against the marks of that moment). Site measured: website/build.

| gate | what | pass | fail | known-failing | findings |
|---|---|---|---|---|---|
| gate-1 | no horizontal scroll, nothing past its box | 52 | 0 | 0 | – |
| gate-2 | always visible | 30 | 0 | 22 | UXF-008 |
| gate-4 | screen-only reader reaches the truth | 24 | 0 | 4 | UXF-009 |
| gate-5 | accessibility (axe) | 52 | 0 | 0 | – |
| gate-5-keyboard | accessibility (keyboard only) | 24 | 0 | 4 | UXF-009 |
| gate-3 | unmissable completion | – | – | – | docs: the truth in view at the end of gate 4 |
| gate-6 | cue truth | – | – | – | dropped: no guidance cue |

## Per page, viewport and theme

Elements past their box (worst state), always-visible misses (top + end), axe serious/critical nodes.

| page | 1280x800 light | 1280x800 dark | 390x844 light | 390x844 dark |
|---|---|---|---|---|
| landing | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |
| docs | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |
| concepts-capabilities | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |
| concepts-components-and-bindings | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |
| concepts-deployment-targets | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |
| concepts-environment-and-admission | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |
| concepts-observe | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |
| concepts-specification | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |
| examples-rally | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |
| getting-started | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |
| reference-cli (generated) | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |
| status (generated) | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |
| search | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |

## Screen-only reader per task

Pointer plays; `tabs` from the keyboard play at the same viewport and theme.

| task | card | viewport | theme | reached | actions | looks | detours | keyboard reached | tabs | path |
|---|---|---|---|---|---|---|---|---|---|---|
| install | A-newcomer (anchor) | 1280x800 | dark | true | 1 | 2 | 0 | true | 11 | link "Get started →" |
| install | A-newcomer (anchor) | 1280x800 | light | true | 1 | 2 | 0 | true | 11 | link "Get started →" |
| install | A-newcomer (anchor) | 390x844 | dark | true | 5 | 7 | 0 | true | 19 | link "Get started →" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "Build" |
| install | A-newcomer (anchor) | 390x844 | light | true | 5 | 7 | 0 | true | 17 | link "Get started →" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "Build" |
| http | B-integrator (anchor) | 1280x800 | dark | true | 2 | 3 | 1 | true | 27 | link "Get started →" → link "Serving ingress.network over HTTP" |
| http | B-integrator (anchor) | 1280x800 | light | true | 2 | 3 | 1 | true | 30 | link "Get started →" → link "Serving ingress.network over HTTP" |
| http | B-integrator (anchor) | 390x844 | dark | true | 3 | 4 | 1 | true | 21 | link "Get started →" → button "Toggle navigation bar" (reveal) → link "Serving ingress.network over HTTP" |
| http | B-integrator (anchor) | 390x844 | light | true | 3 | 4 | 1 | true | 21 | link "Get started →" → button "Toggle navigation bar" (reveal) → link "Serving ingress.network over HTTP" |
| observe | R00-operator | 1280x800 | dark | true | 1 | 1 | 0 | true | 26 | link "run --observe" |
| observe | R00-operator | 1280x800 | light | true | 1 | 1 | 0 | true | 30 | link "run --observe" |
| observe | R00-operator | 390x844 | dark | false | 30 | 65 | 2 | false | 100 | button "Toggle navigation bar" (reveal) → link "run --observe" → read-scroll ×13 → link "Observe" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "The observe API and UI" → searchbox "Search" (search) → type "--observe" → option "Observe Documentation › Concepts" → link "The observe API and UI" → scroll ×2 → link "docs/design/observe.md" → scroll → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) — stuck: not there after 30 actions |
| observe | R00-operator | 390x844 | light | false | 30 | 65 | 2 | false | 101 | button "Toggle navigation bar" (reveal) → link "run --observe" → read-scroll ×13 → link "Observe" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "The observe API and UI" → searchbox "Search" (search) → type "--observe" → option "Observe Documentation › Concepts" → link "The observe API and UI" → scroll ×2 → link "docs/design/observe.md" → scroll → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) — stuck: not there after 30 actions |
| release | A-newcomer (anchor) | 1280x800 | dark | true | 17 | 42 | 2 | true | 94 | link "Status" → searchbox "Search" (search) → type "release" → option "…cargo build --locked --release -p spindle-cli The … Getting started › Getting started › Build" → read-scroll ×12 → link "Changelog" |
| release | A-newcomer (anchor) | 1280x800 | light | true | 17 | 42 | 2 | true | 94 | link "Status" → searchbox "Search" (search) → type "release" → option "…cargo build --locked --release -p spindle-cli The … Getting started › Getting started › Build" → read-scroll ×12 → link "Changelog" |
| release | A-newcomer (anchor) | 390x844 | dark | true | 21 | 47 | 2 | true | 130 | button "Toggle navigation bar" (reveal) → link "Status" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → searchbox "Search" (search) → type "release" → option "…cargo build --locked --release -p spindle-cli The … Getting started › Getting started › Build" → read-scroll ×12 → link "Changelog" |
| release | A-newcomer (anchor) | 390x844 | light | true | 21 | 47 | 2 | true | 131 | button "Toggle navigation bar" (reveal) → link "Status" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → searchbox "Search" (search) → type "release" → option "…cargo build --locked --release -p spindle-cli The … Getting started › Getting started › Build" → read-scroll ×12 → link "Changelog" |
| environment | B-integrator (anchor) | 1280x800 | dark | true | 3 | 3 | 0 | true | 15 | searchbox "Search" (search) → type "environment document" → option "An environment document (spindle-environment/1, … Documentation › Concepts › Environment and admission › The environment" |
| environment | B-integrator (anchor) | 1280x800 | light | true | 3 | 3 | 0 | true | 15 | searchbox "Search" (search) → type "environment document" → option "An environment document (spindle-environment/1, … Documentation › Concepts › Environment and admission › The environment" |
| environment | B-integrator (anchor) | 390x844 | dark | true | 5 | 5 | 0 | true | 13 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → searchbox "Search" (search) → type "environment document" → option "An environment document (spindle-environment/1, … Documentation › Concepts › Environment and admission › The environment" |
| environment | B-integrator (anchor) | 390x844 | light | true | 5 | 5 | 0 | true | 13 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → searchbox "Search" (search) → type "environment document" → option "An environment document (spindle-environment/1, … Documentation › Concepts › Environment and admission › The environment" |
| http-call | R01-caller | 1280x800 | dark | true | 3 | 3 | 0 | true | 18 | searchbox "Search" (search) → type "HTTP call" → option "An external cause bound to an HTTP call Documentation › Concepts › Environment and admission" |
| http-call | R01-caller | 1280x800 | light | true | 3 | 3 | 0 | true | 18 | searchbox "Search" (search) → type "HTTP call" → option "An external cause bound to an HTTP call Documentation › Concepts › Environment and admission" |
| http-call | R01-caller | 390x844 | dark | true | 5 | 5 | 0 | true | 72 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → searchbox "Search" (search) → type "HTTP call" → option "An external cause bound to an HTTP call Documentation › Concepts › Environment and admission" |
| http-call | R01-caller | 390x844 | light | true | 5 | 5 | 0 | true | 72 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → searchbox "Search" (search) → type "HTTP call" → option "An external cause bound to an HTTP call Documentation › Concepts › Environment and admission" |
| topology | R01-caller | 1280x800 | dark | true | 1 | 1 | 0 | true | 26 | link "run --observe" |
| topology | R01-caller | 1280x800 | light | true | 1 | 1 | 0 | true | 26 | link "run --observe" |
| topology | R01-caller | 390x844 | dark | false | 30 | 71 | 1 | false | 130 | button "Toggle navigation bar" (reveal) → link "run --observe" → button "Toggle navigation bar" (reveal) → link "Observe" → button "On this page" (reveal) → link "The observe API and UI" → searchbox "Search" (search) → type "topology" → option "…Vue 3 app in ui/ with topology, instance and view … Status › Status" → read-scroll ×20 → button "Toggle navigation bar" (reveal) — stuck: not there after 30 actions |
| topology | R01-caller | 390x844 | light | false | 30 | 71 | 1 | false | 128 | button "Toggle navigation bar" (reveal) → link "run --observe" → button "Toggle navigation bar" (reveal) → link "Observe" → button "On this page" (reveal) → link "The observe API and UI" → searchbox "Search" (search) → type "topology" → option "…Vue 3 app in ui/ with topology, instance and view … Status › Status" → read-scroll ×20 → button "Toggle navigation bar" (reveal) — stuck: not there after 30 actions |

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
| install | 5 (base 5, last 5) | 7 (base 7, last 7) | 0 (base 0, last 0) | 2 (base 2, last 2) | 2 (base 2, last 2) | 19 (base 15, last 15) |
| http | 3 (base 3, last 3) | 4 (base 4, last 4) | 1 (base 1, last 1) | 1.5 (base 1.5, last 1.5) | 2 (base 2, last 2) | 30 (base 25, last 35) |
| observe | 31 (base 31, last 31) | 93 (base 93, last 93) | 2 (base 2, last 1) | 3 (base 3, last 3) | 3 (base 3, last 3) | 400 (base 400, last 400) |
| release | 21 (base 26, last 26) | 47 (base 68, last 68) | 2 (base 1, last 1) | 3 (base 3, last 3) | 3 (base 3, last 3) | 131 (base 134, last 134) |
| environment | 5 (base 6, last 6) | 5 (base 13, last 13) | 0 (base 0, last 0) | 1 (base 3, last 3) | 1 (base 3, last 3) | 15 (base 14, last 14) |
| http-call | 5 (base –, last 24) | 5 (base –, last 53) | 0 (base –, last 1) | 1 (base –, last 2.5) | 1 (base –, last 3) | 72 (base –, last 181) |
| topology | 31 (base –, last 31) | 93 (base –, last 93) | 1 (base –, last 2) | 3 (base –, last 3) | 3 (base –, last 3) | 400 (base –, last 400) |

| page | axe.light | axe.dark | overflow.elements | always_visible.misses | layout_shift.score | reading_load.words | reading_load.long_paragraphs | reading_load.max_paragraph |
|---|---|---|---|---|---|---|---|---|
| landing | 0 (base 0) | 0 (base 0) | 0 (base 0) | 0 (base 4) | 0 (base –) | 2249 (base 2249) | 0 (base 0) | 51 (base 51) |
| docs | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 5) | 0 (base –) | 459 (base 459) | 1 (base 1) | 84 (base 84) |
| concepts-capabilities | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 5) | 0 (base –) | 1062 (base 1049) | 2 (base 2) | 213 (base 213) |
| concepts-components-and-bindings | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 5) | 0 (base –) | 506 (base 506) | 1 (base 1) | 83 (base 83) |
| concepts-deployment-targets | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 5) | 0 (base –) | 485 (base 485) | 0 (base 0) | 59 (base 59) |
| concepts-environment-and-admission | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 5) | 0 (base –) | 831 (base 511) | 2 (base 0) | 121 (base 68) |
| concepts-observe | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 5) | 0 (base –) | 936 (base 936) | 2 (base 2) | 155 (base 155) |
| concepts-specification | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 5) | 0 (base –) | 987 (base 987) | 4 (base 4) | 181 (base 181) |
| examples-rally | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 5) | 0 (base –) | 1025 (base 1025) | 2 (base 2) | 194 (base 194) |
| getting-started | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 5) | 0 (base –) | 1251 (base 1233) | 0 (base 0) | 74 (base 74) |
| reference-cli | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 5) | 0 (base –) | 3549 (base 3358) | 12 (base 11) | 322 (base 322) |
| status | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 5) | 0 (base –) | 2791 (base 2791) | 0 (base 0) | 51 (base 51) |
| search | 0 (base –) | 0 (base –) | 0 (base –) | 0 (base –) | 0 (base –) | 53 (base –) | 0 (base –) | 5 (base –) |

Reading load: median 987 words per page, 26 paragraphs over 80 words in all.

## Worst pages

| viewport | theme | page | gate failures | shot |
|---|---|---|---|---|
| 1280x800 | light | reference-cli | 1 | [worst--reference-cli--1280x800--light.png](shots/worst--reference-cli--1280x800--light.png) |
| 1280x800 | dark | reference-cli | 1 | [worst--reference-cli--1280x800--dark.png](shots/worst--reference-cli--1280x800--dark.png) |
| 390x844 | light | reference-cli | 0 | [worst--reference-cli--390x844--light.png](shots/worst--reference-cli--390x844--light.png) |
| 390x844 | dark | reference-cli | 0 | [worst--reference-cli--390x844--dark.png](shots/worst--reference-cli--390x844--dark.png) |

## Contact sheets

- [shots/contact--1280x800--light.jpg](shots/contact--1280x800--light.jpg): every page at the top, after load and settle.
- [shots/contact--1280x800--dark.jpg](shots/contact--1280x800--dark.jpg): every page at the top, after load and settle.
- [shots/contact--390x844--light.jpg](shots/contact--390x844--light.jpg): every page at the top, after load and settle.
- [shots/contact--390x844--dark.jpg](shots/contact--390x844--dark.jpg): every page at the top, after load and settle.

## Measured-only viewports

Reported, not gated: elements past their box, always-visible misses and axe serious/critical nodes, worst over the themes.

| page | 1440x900 | 1024x768 | 768x1024 |
|---|---|---|---|
| landing | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |
| docs | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 |
| concepts-capabilities | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 |
| concepts-components-and-bindings | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 |
| concepts-deployment-targets | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 |
| concepts-environment-and-admission | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 |
| concepts-observe | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 |
| concepts-specification | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 |
| examples-rally | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 |
| getting-started | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 |
| reference-cli | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 |
| status | 0 / 1 / 0 | 0 / 1 / 0 | 0 / 0 / 0 |
| search | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |

## Not measured in this round

- Questionnaire scores: none, no people (L6 not run); the cards were played by the screen-only reader.
- L5 discoverability probe misses: not run (paid, operator-only). L4 keyed replays: not applicable (a static site).
- L0 and gate 6: dropped, no guidance cue. Gate 3 is the truth in view at the end of gate 4.
- Recordings (rrweb): none; the evidence is the page checks, the plays and the shots.

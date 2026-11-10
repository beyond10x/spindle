# R-02 after round report

Date 2026-11-10, commit `82faeb87+uncommitted`. Spindle documentation site (website/, Docusaurus). Hermetic: the built site served from `website/build` on 127.0.0.1, every request off the site answered by a stub page; no keys.
Page checks: 52 (13 sitemap pages × 2 gating viewports × 2 themes), measured-only: 78. Screen-only plays: 64 (8 tasks × 2 viewports × 2 themes × pointer and keyboard).

## Hard gates

Verdicts over every gating check, judged again against `ux/known-failing.json` as it stands; known-failing = covered by a mark with its open finding. As run, 0 of 220 verdicts were red (against the marks of that moment). Site measured: website/build.

| gate | what | pass | fail | known-failing | findings |
|---|---|---|---|---|---|
| gate-1 | no horizontal scroll, nothing past its box | 52 | 0 | 0 | – |
| gate-2 | always visible | 30 | 0 | 22 | UXF-008 |
| gate-4 | screen-only reader reaches the truth | 32 | 0 | 0 | – |
| gate-5 | accessibility (axe) | 52 | 0 | 0 | – |
| gate-5-keyboard | accessibility (keyboard only) | 30 | 0 | 2 | UXF-020 |
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
| install | A-newcomer (anchor) | 1280x800 | dark | true | 1 | 2 | 0 | true | 21 | link "Get started →" |
| install | A-newcomer (anchor) | 1280x800 | light | true | 1 | 2 | 0 | true | 11 | link "Get started →" |
| install | A-newcomer (anchor) | 390x844 | dark | true | 5 | 7 | 0 | true | 17 | link "Get started →" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "Build" |
| install | A-newcomer (anchor) | 390x844 | light | true | 5 | 7 | 0 | true | 17 | link "Get started →" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "Build" |
| http | B-integrator (anchor) | 1280x800 | dark | true | 2 | 3 | 1 | true | 27 | link "Get started →" → link "Serving ingress.network over HTTP" |
| http | B-integrator (anchor) | 1280x800 | light | true | 2 | 3 | 1 | true | 27 | link "Get started →" → link "Serving ingress.network over HTTP" |
| http | B-integrator (anchor) | 390x844 | dark | true | 3 | 4 | 1 | true | 21 | link "Get started →" → button "Toggle navigation bar" (reveal) → link "Serving ingress.network over HTTP" |
| http | B-integrator (anchor) | 390x844 | light | true | 3 | 4 | 1 | true | 22 | link "Get started →" → button "Toggle navigation bar" (reveal) → link "Serving ingress.network over HTTP" |
| observe | R00-operator | 1280x800 | dark | true | 1 | 1 | 0 | true | 27 | link "run --observe" |
| observe | R00-operator | 1280x800 | light | true | 1 | 1 | 0 | true | 26 | link "run --observe" |
| observe | R00-operator | 390x844 | dark | true | 2 | 2 | 0 | true | 20 | button "Toggle navigation bar" (reveal) → link "run --observe" |
| observe | R00-operator | 390x844 | light | true | 2 | 2 | 0 | true | 27 | button "Toggle navigation bar" (reveal) → link "run --observe" |
| release | A-newcomer (anchor) | 1280x800 | dark | true | 17 | 42 | 2 | true | 94 | link "Status" → searchbox "Search" (search) → type "release" → option "…cargo build --locked --release -p spindle-cli The … Getting started › Getting started › Build" → read-scroll ×12 → link "Changelog" |
| release | A-newcomer (anchor) | 1280x800 | light | true | 17 | 42 | 2 | true | 94 | link "Status" → searchbox "Search" (search) → type "release" → option "…cargo build --locked --release -p spindle-cli The … Getting started › Getting started › Build" → read-scroll ×12 → link "Changelog" |
| release | A-newcomer (anchor) | 390x844 | dark | true | 21 | 47 | 2 | true | 171 | button "Toggle navigation bar" (reveal) → link "Status" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → searchbox "Search" (search) → type "release" → option "…cargo build --locked --release -p spindle-cli The … Getting started › Getting started › Build" → read-scroll ×12 → link "Changelog" |
| release | A-newcomer (anchor) | 390x844 | light | true | 21 | 47 | 2 | true | 215 | button "Toggle navigation bar" (reveal) → link "Status" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → searchbox "Search" (search) → type "release" → option "…cargo build --locked --release -p spindle-cli The … Getting started › Getting started › Build" → read-scroll ×12 → link "Changelog" |
| environment | B-integrator (anchor) | 1280x800 | dark | true | 3 | 3 | 0 | true | 15 | searchbox "Search" (search) → type "environment document" → option "An environment document (spindle-environment/1, … Documentation › Concepts › Environment and admission › The environment" |
| environment | B-integrator (anchor) | 1280x800 | light | true | 3 | 3 | 0 | true | 15 | searchbox "Search" (search) → type "environment document" → option "An environment document (spindle-environment/1, … Documentation › Concepts › Environment and admission › The environment" |
| environment | B-integrator (anchor) | 390x844 | dark | true | 5 | 5 | 0 | true | 90 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → searchbox "Search" (search) → type "environment document" → option "An environment document (spindle-environment/1, … Documentation › Concepts › Environment and admission › The environment" |
| environment | B-integrator (anchor) | 390x844 | light | true | 5 | 5 | 0 | true | 90 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → searchbox "Search" (search) → type "environment document" → option "An environment document (spindle-environment/1, … Documentation › Concepts › Environment and admission › The environment" |
| http-call | R01-caller | 1280x800 | dark | true | 3 | 3 | 0 | true | 18 | searchbox "Search" (search) → type "HTTP call" → option "An external cause bound to an HTTP call Documentation › Concepts › Environment and admission" |
| http-call | R01-caller | 1280x800 | light | true | 3 | 3 | 0 | true | 18 | searchbox "Search" (search) → type "HTTP call" → option "An external cause bound to an HTTP call Documentation › Concepts › Environment and admission" |
| http-call | R01-caller | 390x844 | dark | true | 5 | 5 | 0 | true | 93 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → searchbox "Search" (search) → type "HTTP call" → option "An external cause bound to an HTTP call Documentation › Concepts › Environment and admission" |
| http-call | R01-caller | 390x844 | light | true | 5 | 5 | 0 | true | 93 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → searchbox "Search" (search) → type "HTTP call" → option "An external cause bound to an HTTP call Documentation › Concepts › Environment and admission" |
| topology | R01-caller | 1280x800 | dark | true | 1 | 1 | 0 | true | 30 | link "run --observe" |
| topology | R01-caller | 1280x800 | light | true | 1 | 1 | 0 | true | 28 | link "run --observe" |
| topology | R01-caller | 390x844 | dark | true | 2 | 2 | 0 | true | 26 | button "Toggle navigation bar" (reveal) → link "run --observe" |
| topology | R01-caller | 390x844 | light | true | 2 | 2 | 0 | true | 32 | button "Toggle navigation bar" (reveal) → link "run --observe" |
| request-log-size | R02-phone-operator | 1280x800 | dark | true | 11 | 21 | 1 | true | 189 | link "Observe" → link "The observe API and UI" → link "run --observe" → searchbox "Search" (search) → type "request records" → Escape (no result off this page) → scroll ×5 |
| request-log-size | R02-phone-operator | 1280x800 | light | true | 11 | 21 | 1 | true | 186 | link "Observe" → link "The observe API and UI" → link "run --observe" → searchbox "Search" (search) → type "request records" → Escape (no result off this page) → scroll ×5 |
| request-log-size | R02-phone-operator | 390x844 | dark | true | 14 | 24 | 1 | false | 278 | button "Toggle navigation bar" (reveal) → link "Observe" → button "Toggle navigation bar" (reveal) → link "run --observe" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → searchbox "Search" (search) → type "request records" → Escape (no result off this page) → scroll ×5 |
| request-log-size | R02-phone-operator | 390x844 | light | true | 14 | 24 | 1 | false | 277 | button "Toggle navigation bar" (reveal) → link "Observe" → button "Toggle navigation bar" (reveal) → link "run --observe" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → searchbox "Search" (search) → type "request records" → Escape (no result off this page) → scroll ×5 |

Truth per task:

- `install` "Find the install command": `/docs/getting-started` #build, text "cargo build --locked --release -p spindle-cli"; words "install", "get started", "getting started", "build"; start `/`.
- `http` "Find how to run a specification over HTTP": `/docs/concepts/capabilities` #serving-ingressnetwork-over-http, text "over HTTP on"; words "HTTP", "serve", "ingress", "run"; start `/`.
- `observe` "Find what run --observe serves": `/docs/reference/cli` #run, text "Record every request the listener answers outside"; words "--observe", "observe", "run"; start `/docs/examples/rally`.
- `release` "Find the release and version status": off the site, https://github.com/beyond10x/spindle/blob/main/CHANGELOG.md; words "release", "version", "changelog", "status"; start `/`.
- `environment` "Find what an environment document is": `/docs/concepts/environment-and-admission` #the-environment, text "An environment document"; words "environment document", "environment", "admission"; start `/`.
- `http-call` "Find how to answer an external cause with an outbound HTTP call": `/docs/concepts/environment-and-admission` #an-external-cause-bound-to-an-http-call, text "An environment may declare HTTP calls"; words "HTTP call", "external", "HTTP", "environment"; start `/`.
- `topology` "Find what the topology of an observed run answers": `/docs/reference/cli` #run, text "graph derived from the specification"; words "topology", "--observe", "observe"; start `/docs/concepts/observe`.
- `request-log-size` "Find how many request records an observed run keeps, and how to change it": `/docs/reference/cli` #run--observe-requests, text "How many request records"; words "request records", "--observe-requests", "observe"; start `/docs/getting-started`.

## Metrics against the bars

Every key lower-is-better; `base` is `ux/baseline.json`, `last` the previous round. Task keys: worst over the gating viewports and themes.

| scenario | screen_only.actions | screen_only.looks | screen_only.detours | glance_cost.median | glance_cost.p95 | keyboard.tabs |
|---|---|---|---|---|---|---|
| install | 5 (base 5, last 5) | 7 (base 7, last 7) | 0 (base 0, last 0) | 2 (base 2, last 2) | 2 (base 2, last 2) | 21 (base 19, last 18) |
| http | 3 (base 3, last 3) | 4 (base 4, last 4) | 1 (base 1, last 1) | 1.5 (base 1.5, last 1.5) | 2 (base 2, last 2) | 27 (base 30, last 27) |
| observe | 2 (base 31, last 31) | 2 (base 93, last 93) | 0 (base 2, last 2) | 1 (base 3, last 3) | 1 (base 3, last 3) | 27 (base 400, last 400) |
| release | 21 (base 21, last 21) | 47 (base 47, last 47) | 2 (base 2, last 2) | 3 (base 3, last 3) | 3 (base 3, last 3) | 215 (base 131, last 131) |
| environment | 5 (base 5, last 5) | 5 (base 5, last 5) | 0 (base 0, last 0) | 1 (base 1, last 1) | 1 (base 1, last 1) | 90 (base 15, last 15) |
| http-call | 5 (base 5, last 5) | 5 (base 5, last 5) | 0 (base 0, last 0) | 1 (base 1, last 1) | 1 (base 1, last 1) | 93 (base 72, last 72) |
| topology | 2 (base 31, last 31) | 2 (base 93, last 93) | 0 (base 1, last 1) | 1 (base 3, last 3) | 1 (base 3, last 3) | 32 (base 400, last 400) |
| request-log-size | 14 (base –, last 31) | 24 (base –, last 93) | 1 (base –, last 1) | 1 (base –, last 3) | 3 (base –, last 3) | 400 (base –, last 400) |

| page | axe.light | axe.dark | overflow.elements | always_visible.misses | layout_shift.score | reading_load.words | reading_load.long_paragraphs | reading_load.max_paragraph |
|---|---|---|---|---|---|---|---|---|
| landing | 0 (base 0) | 0 (base 0) | 0 (base 0) | 0 (base 0) | 0 (base 0) | 2249 (base 2249) | 0 (base 0) | 51 (base 51) |
| docs | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 444 (base 459) | 1 (base 1) | 84 (base 84) |
| concepts-capabilities | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 1049 (base 1062) | 2 (base 2) | 213 (base 213) |
| concepts-components-and-bindings | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 493 (base 506) | 1 (base 1) | 83 (base 83) |
| concepts-deployment-targets | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 485 (base 485) | 0 (base 0) | 59 (base 59) |
| concepts-environment-and-admission | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 818 (base 831) | 2 (base 2) | 121 (base 121) |
| concepts-observe | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0.005 (base 0) | 923 (base 936) | 2 (base 2) | 155 (base 155) |
| concepts-specification | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 987 (base 987) | 4 (base 4) | 181 (base 181) |
| examples-rally | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 999 (base 1025) | 2 (base 2) | 194 (base 194) |
| getting-started | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 1249 (base 1251) | 0 (base 0) | 74 (base 74) |
| reference-cli | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 3445 (base 3549) | 12 (base 12) | 322 (base 322) |
| status | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 2778 (base 2791) | 0 (base 0) | 51 (base 51) |
| search | 0 (base 0) | 0 (base 0) | 0 (base 0) | 0 (base 0) | 0 (base 0) | 53 (base 53) | 0 (base 0) | 5 (base 5) |

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

## Caveats

- This is the second after-measure. The first (221 of 223) had gate 5 (keyboard) of `request-log-size` at 390x844 red in both themes; filed as UXF-020 and marked (`../triage.md`).
- Tool changes since the R-02 measure: T12 (the keyboard reader takes a `combobox` search field) and T5 (a state signature counts Tab stops). They raise `keyboard.tabs` of environment, http-call, install and release (decision `R-02` in `baseline.json`).
- Not run: L4 and L5 (paid, no keys), L6 and L8 (no people).

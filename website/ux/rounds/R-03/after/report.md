# R-03 after round report

Date 2026-11-22, commit `8591d014+uncommitted`. Spindle documentation site (website/, Docusaurus). Hermetic: the built site served from `website/build` on 127.0.0.1, every request off the site answered by a stub page; no keys.
Page checks: 52 (13 sitemap pages × 2 gating viewports × 2 themes), measured-only: 78. Screen-only plays: 64 (8 tasks × 2 viewports × 2 themes × pointer and keyboard).

## Hard gates

Verdicts over every gating check, judged again against `ux/known-failing.json` as it stands; known-failing = covered by a mark with its open finding. As run, 0 of 220 verdicts were red (against the marks of that moment). Site measured: website/build.

| gate | what | pass | fail | known-failing | findings |
|---|---|---|---|---|---|
| gate-1 | no horizontal scroll, nothing past its box | 52 | 0 | 0 | – |
| gate-2 | always visible | 30 | 0 | 22 | UXF-008 |
| gate-4 | screen-only reader reaches the truth | 32 | 0 | 0 | – |
| gate-5 | accessibility (axe) | 52 | 0 | 0 | – |
| gate-5-keyboard | accessibility (keyboard only) | 32 | 0 | 0 | – |
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
| install | A-newcomer (anchor) | 1280x800 | dark | true | 2 | 3 | 0 | true | 86 | link "Get started →" → link "Build" |
| install | A-newcomer (anchor) | 1280x800 | light | true | 2 | 3 | 0 | true | 86 | link "Get started →" → link "Build" |
| install | A-newcomer (anchor) | 390x844 | dark | true | 5 | 7 | 0 | true | 17 | link "Get started →" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "Build" |
| install | A-newcomer (anchor) | 390x844 | light | true | 5 | 7 | 0 | true | 17 | link "Get started →" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → button "On this page" (reveal) → link "Build" |
| http | B-integrator (anchor) | 1280x800 | dark | true | 2 | 3 | 1 | true | 27 | link "Get started →" → link "Serving ingress.network over HTTP" |
| http | B-integrator (anchor) | 1280x800 | light | true | 2 | 3 | 1 | true | 27 | link "Get started →" → link "Serving ingress.network over HTTP" |
| http | B-integrator (anchor) | 390x844 | dark | true | 3 | 4 | 1 | true | 21 | link "Get started →" → button "Toggle navigation bar" (reveal) → link "Serving ingress.network over HTTP" |
| http | B-integrator (anchor) | 390x844 | light | true | 3 | 4 | 1 | true | 21 | link "Get started →" → button "Toggle navigation bar" (reveal) → link "Serving ingress.network over HTTP" |
| observe | R00-operator | 1280x800 | dark | true | 1 | 1 | 0 | true | 21 | link "run --observe" |
| observe | R00-operator | 1280x800 | light | true | 1 | 1 | 0 | true | 21 | link "run --observe" |
| observe | R00-operator | 390x844 | dark | true | 2 | 2 | 0 | true | 20 | button "Toggle navigation bar" (reveal) → link "run --observe" |
| observe | R00-operator | 390x844 | light | true | 2 | 2 | 0 | true | 20 | button "Toggle navigation bar" (reveal) → link "run --observe" |
| release | A-newcomer (anchor) | 1280x800 | dark | true | 2 | 3 | 1 | true | 35 | link "Status" → link "CHANGELOG.md" |
| release | A-newcomer (anchor) | 1280x800 | light | true | 2 | 3 | 1 | true | 35 | link "Status" → link "CHANGELOG.md" |
| release | A-newcomer (anchor) | 390x844 | dark | true | 3 | 4 | 1 | true | 21 | button "Toggle navigation bar" (reveal) → link "Status" → link "CHANGELOG.md" |
| release | A-newcomer (anchor) | 390x844 | light | true | 3 | 4 | 1 | true | 21 | button "Toggle navigation bar" (reveal) → link "Status" → link "CHANGELOG.md" |
| environment | B-integrator (anchor) | 1280x800 | dark | true | 3 | 3 | 0 | true | 14 | searchbox "Search" (search) → type "environment document" → option "An environment document (spindle-environment/1, … Documentation › Concepts › Environment and admission › The environment" |
| environment | B-integrator (anchor) | 1280x800 | light | true | 3 | 3 | 0 | true | 14 | searchbox "Search" (search) → type "environment document" → option "An environment document (spindle-environment/1, … Documentation › Concepts › Environment and admission › The environment" |
| environment | B-integrator (anchor) | 390x844 | dark | true | 5 | 5 | 0 | true | 96 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → searchbox "Search" (search) → type "environment document" → option "An environment document (spindle-environment/1, … Documentation › Concepts › Environment and admission › The environment" |
| environment | B-integrator (anchor) | 390x844 | light | true | 5 | 5 | 0 | true | 96 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → searchbox "Search" (search) → type "environment document" → option "An environment document (spindle-environment/1, … Documentation › Concepts › Environment and admission › The environment" |
| http-call | R01-caller | 1280x800 | dark | true | 3 | 3 | 0 | true | 19 | searchbox "Search" (search) → type "HTTP call" → option "An external cause bound to an HTTP call Documentation › Concepts › Environment and admission" |
| http-call | R01-caller | 1280x800 | light | true | 3 | 3 | 0 | true | 19 | searchbox "Search" (search) → type "HTTP call" → option "An external cause bound to an HTTP call Documentation › Concepts › Environment and admission" |
| http-call | R01-caller | 390x844 | dark | true | 5 | 5 | 0 | true | 101 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → searchbox "Search" (search) → type "HTTP call" → option "An external cause bound to an HTTP call Documentation › Concepts › Environment and admission" |
| http-call | R01-caller | 390x844 | light | true | 5 | 5 | 0 | true | 101 | button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → searchbox "Search" (search) → type "HTTP call" → option "An external cause bound to an HTTP call Documentation › Concepts › Environment and admission" |
| topology | R01-caller | 1280x800 | dark | true | 1 | 1 | 0 | true | 21 | link "run --observe" |
| topology | R01-caller | 1280x800 | light | true | 1 | 1 | 0 | true | 21 | link "run --observe" |
| topology | R01-caller | 390x844 | dark | true | 2 | 2 | 0 | true | 20 | button "Toggle navigation bar" (reveal) → link "run --observe" |
| topology | R01-caller | 390x844 | light | true | 2 | 2 | 0 | true | 20 | button "Toggle navigation bar" (reveal) → link "run --observe" |
| request-log-size | R02-phone-operator | 1280x800 | dark | true | 7 | 8 | 1 | true | 221 | link "Observe" → link "The observe API and UI" → link "run --observe" → searchbox "Search" (search) → type "request records" → option "… with the last 256 request records, newest first… Documentation › Concepts › Observe › A served run's metrics and request log" → link "--observe-requests" |
| request-log-size | R02-phone-operator | 1280x800 | light | true | 7 | 8 | 1 | true | 221 | link "Observe" → link "The observe API and UI" → link "run --observe" → searchbox "Search" (search) → type "request records" → option "… with the last 256 request records, newest first… Documentation › Concepts › Observe › A served run's metrics and request log" → link "--observe-requests" |
| request-log-size | R02-phone-operator | 390x844 | dark | true | 10 | 11 | 1 | true | 213 | button "Toggle navigation bar" (reveal) → link "Observe" → button "Toggle navigation bar" (reveal) → link "run --observe" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → searchbox "Search" (search) → type "request records" → option "… with the last 256 request records, newest first… Documentation › Concepts › Observe › A served run's metrics and request log" → link "--observe-requests" |
| request-log-size | R02-phone-operator | 390x844 | light | true | 10 | 11 | 1 | true | 213 | button "Toggle navigation bar" (reveal) → link "Observe" → button "Toggle navigation bar" (reveal) → link "run --observe" → button "Toggle navigation bar" (reveal) → button "Close navigation bar" (dismiss) → searchbox "Search" (search) → type "request records" → option "… with the last 256 request records, newest first… Documentation › Concepts › Observe › A served run's metrics and request log" → link "--observe-requests" |

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
| install | 5 (base 5, last 5) | 7 (base 7, last 7) | 0 (base 0, last 0) | 1.5 (base 1.5, last 1.5) | 2 (base 2, last 2) | 86 (base 85, last 86) |
| http | 3 (base 3, last 3) | 4 (base 4, last 4) | 1 (base 1, last 1) | 1.5 (base 1.5, last 1.5) | 2 (base 2, last 2) | 27 (base 29, last 27) |
| observe | 2 (base 2, last 2) | 2 (base 2, last 2) | 0 (base 0, last 0) | 1 (base 1, last 1) | 1 (base 1, last 1) | 21 (base 28, last 21) |
| release | 3 (base 3, last 26) | 4 (base 4, last 62) | 1 (base 1, last 2) | 1.5 (base 1.5, last 3) | 2 (base 2, last 3) | 35 (base 35, last 400) |
| environment | 5 (base 5, last 5) | 5 (base 5, last 5) | 0 (base 0, last 0) | 1 (base 1, last 1) | 1 (base 1, last 1) | 96 (base 90, last 96) |
| http-call | 5 (base 5, last 5) | 5 (base 5, last 5) | 0 (base 0, last 0) | 1 (base 1, last 1) | 1 (base 1, last 1) | 101 (base 93, last 101) |
| topology | 2 (base 2, last 2) | 2 (base 2, last 2) | 0 (base 0, last 0) | 1 (base 1, last 1) | 1 (base 1, last 1) | 21 (base 21, last 21) |
| request-log-size | 10 (base 10, last 31) | 11 (base 11, last 93) | 1 (base 1, last 3) | 1 (base 1, last 3) | 2 (base 2, last 3) | 221 (base 221, last 400) |

| page | axe.light | axe.dark | overflow.elements | always_visible.misses | layout_shift.score | reading_load.words | reading_load.long_paragraphs | reading_load.max_paragraph |
|---|---|---|---|---|---|---|---|---|
| landing | 0 (base 0) | 0 (base 0) | 0 (base 0) | 0 (base 0) | 0 (base 0) | 2890 (base 2890) | 0 (base 0) | 51 (base 51) |
| docs | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 461 (base 461) | 0 (base 0) | 75 (base 75) |
| concepts-capabilities | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 1328 (base 1328) | 2 (base 2) | 184 (base 184) |
| concepts-components-and-bindings | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 511 (base 511) | 1 (base 1) | 83 (base 83) |
| concepts-deployment-targets | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 480 (base 480) | 0 (base 0) | 59 (base 59) |
| concepts-environment-and-admission | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 2869 (base 2869) | 1 (base 1) | 83 (base 83) |
| concepts-observe | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 1524 (base 1524) | 0 (base 0) | 78 (base 78) |
| concepts-specification | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 1759 (base 1759) | 1 (base 1) | 82 (base 82) |
| examples-rally | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 1058 (base 1058) | 0 (base 0) | 80 (base 80) |
| getting-started | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 1375 (base 1375) | 1 (base 1) | 91 (base 91) |
| reference-cli | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 4953 (base 4953) | 17 (base 17) | 408 (base 408) |
| status | 0 (base 0) | 0 (base 0) | 0 (base 0) | 1 (base 1) | 0 (base 0) | 3611 (base 3611) | 0 (base 0) | 51 (base 51) |
| search | 0 (base 0) | 0 (base 0) | 0 (base 0) | 0 (base 0) | 0 (base 0) | 53 (base 53) | 0 (base 0) | 5 (base 5) |

Reading load: median 1375 words per page, 23 paragraphs over 80 words in all.

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

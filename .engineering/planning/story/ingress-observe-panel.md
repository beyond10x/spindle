---
format: aep.planning-md/3
id: story:ingress-observe-panel
kind: story
status: implemented
title: An observed run serves one embeddable panel page
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability
- depends_on: story:observe-ingress-model
- depends_on: story:ingress-request-log
scope:
- confidence: cited
  path: Taskfile.yml
- confidence: inferred
  path: crates/spindle-cli/build.rs
- confidence: inferred
  path: crates/spindle-cli/tests/observe_panel.rs
- confidence: inferred
  path: crates/spindle-ingress/src/observe.rs
- confidence: inferred
  path: ui/dev/inline.ts
- confidence: cited
  path: ui/package.json
- confidence: inferred
  path: ui/panel.html
- confidence: inferred
  path: ui/src/panel
- confidence: cited
  path: ui/vite.config.ts
- confidence: inferred
  path: ui/vite.panel.config.ts
revision: 16
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T14:45:47Z", actor: "human:timo", revision: 14, decided_on: {"recorded":{"review_outcome":1}}}
- {from: "proposed", to: "active", at: "2026-11-06T14:45:47Z", actor: "human:timo", revision: 15, decided_on: {"recorded":{"review_outcome":1}}}
- {from: "active", to: "implemented", at: "2026-11-07T17:09:20Z", actor: "human:timo", revision: 16, decided_on: {"recorded":{"test_result":1,"review_outcome":2,"verification":1}}}
---
# An observed run serves one embeddable panel page

## Outcome

With `--observe`, `GET /__spindle/ui` answers one HTML page that holds all of its script and
style inline and loads nothing else. It may be framed by any page: no `X-Frame-Options`, no
`frame-ancestors`. Its `Content-Security-Policy` allows inline script and style and `connect-src
'self'` only. It shows a one-line banner (served, not served, refused, not granted, unrouted,
from `RequestLog.totals`) and the request list with a "not served only" toggle. It reads
`./requests` and `./metrics` relative to itself, so it works behind any mount point.

It is Vue 3 and TypeScript, built by a second Vite config (`ui/vite.panel.config.ts`) whose small
local plugin inlines the emitted chunk and stylesheet into `panel.html`, out to `ui/dist-panel/`.
It uses the generated `spindle.observe` types. It is embedded at compile time by a `build.rs`, as
`crates/spindle-control/build.rs` embeds `ui/dist`; without the built page the route answers `503`
naming it.

The Rust tests of this story live in their own file, `crates/spindle-cli/tests/observe_panel.rs`,
apart from the `observe.rs` the other stories of the epic share.

## Acceptance

In `crates/spindle-cli/tests/observe_panel.rs`:

- `the_panel_is_one_self_contained_page`: `GET /__spindle/ui` is `200 text/html`; the body has no
  `src=` or `href=` to anything but `#` or `data:`, and no `<link rel="stylesheet">`.
- `the_panel_may_be_framed`: no `X-Frame-Options`; the CSP has no `frame-ancestors` and limits
  `connect-src` to `'self'`.
- `without_the_built_page_the_panel_answers_503`.

In `ui/src/panel` (vitest):

- `the banner reads its counts from the totals`: a `RequestLog` fixture renders the five counts in
  one line.
- `the not-served toggle asks for class=not-served`.
- `the panel only reads reserved routes relative to itself`: every request it makes is
  `./requests`, `./metrics` or (once `story:live-topology` adds its screen) `./topology`.

Build: `task check:ui` builds both bundles; `pnpm check` passes.

## Scope

- `ui/package.json`, `ui/vite.config.ts` — cited: a `build:panel` script beside `build`.
- `ui/vite.panel.config.ts`, `ui/panel.html`, `ui/src/panel/`, `ui/dev/inline.ts` — inferred,
  new.
- `crates/spindle-cli/build.rs` — inferred, new; the page is handed to the ingress as a
  `&'static str`, so `spindle-ingress` stays free of build assets.
- `crates/spindle-ingress/src/observe.rs` — inferred: the `ui` arm (one line, shared).
- `crates/spindle-cli/tests/observe_panel.rs` — inferred, new: this story's Rust tests; under
  `tests/spindle-cli/` if `story:one-test-binary-per-crate` lands first.
- `Taskfile.yml` — cited: `build:ui` and `check:ui` build the panel too.

## Order

After `story:observe-ingress-model` and `story:ingress-request-log`. Parallel with
`story:ingress-prometheus-metrics`. Before `story:external-answers-observed` (shared
`observe.rs`). `story:live-topology` adds a screen to this page.

## Origin

The operator's "a little UI that can be embedded", and consumer asks (4) and (5), 2026-11-06,
condensed (see the epic).

---
format: aep.planning-md/3
id: story:console-routes-off
kind: story
status: draft
title: An exposed run serves no console route unless asked
summary: One flag or environment entry turns /api/reset, /api/events, /api/model and /api/served off on every listener.
relations:
- serves: vision:spindle
- decomposes: epic:production-service
scope:
- confidence: inferred
  path: AGENTS.md
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: README.md
- confidence: inferred
  path: crates/spindle-cli
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: inferred
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/console_off.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
- confidence: inferred
  path: crates/spindle-ingress/src/surface.rs
- confidence: inferred
  path: website/docs/reference/cli.md
revision: 9
---
# An exposed run serves no console route unless asked

## Outcome

An operator turns the console routes off with one flag or setting: every route of `Console::ALL`
(today `/api/reset`, `/api/events`, `/api/model` and `/api/served`, and any console route a later
story adds) then answers `404` like any unrouted path on a served `run`, whatever secret a request
carries.

## Why

The console routes are added to every surface whose `ingress.network` is bound to
`http-loopback` (`Surface::with_console`, `crates/spindle-ingress/src/surface.rs:1513-1534`;
`Console::ALL`, `crates/spindle-ingress/src/console.rs:30`). Off loopback they require only the
proxy secret (`crates/spindle-cli/src/cli.rs:254-256`), so any proxy that adds the secret to every
request, as a consumer's does, exposes a reset of the whole store and every published event to the
public.

## Acceptance

- `console_routes_off_answer_404_with_the_secret`: a `--trusted-caller-header` loopback run with
  `--console off`; every route of `Console::ALL`, iterated in the test (today `POST /api/reset`,
  `GET /api/events`, `GET /api/model` and `GET /api/served`, and any console route a later story
  adds, such as `story:entity-event-history`'s `GET /api/history`), with the right secret, answers
  `404`, and a row written before is still there.
- `console_routes_off_leaves_the_published_routes_served`: with the console off, a command and a
  view ESS publishes answer as before.
- `console_routes_on_by_default_on_loopback`: without the flag a loopback run serves them as today.
- `console_off_by_setting`: `SPINDLE_SETTING_CONSOLE=off` gives the same `404` as the flag.
- `console_off_by_default_off_loopback`: the function that decides the console answers off for an
  address off loopback without the flag, on with `--console on`, and on for a loopback address
  (a unit test; no test binds off loopback).

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 B: the console is off by default only when `--address` is off loopback
  (`spindle_ingress::exposed`); on loopback it stays on; `--console on` turns it back on.
- D2 A: `/__spindle/` does not follow the switch; it is opt-in through `--observe`
  already.
- D3 A: `run --console <on|off>` with clap `env = "SPINDLE_SETTING_CONSOLE"`; no
  environment-document table.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli` — inferred, the console is added at one call site, `serve.rs:355` (`.with_console(serving.ingress)?`); skipping it there turns all four routes off
- **Files:** `crates/spindle-cli/src/serve.rs:72,355` — cited (read in the tree): `Serving` and the `with_console` call
- **Files:** `crates/spindle-cli/src/cli.rs:222-258` — cited (read in the tree): `run`'s args, where `--console` with `env = "SPINDLE_SETTING_CONSOLE"` goes
- **Files:** `crates/spindle-cli/src/main.rs:190-230,457,560-625` — inferred, the flag threaded through `Command::Run`, `Execute` and `serve::Serving`
- **Files:** `crates/spindle-cli/tests/spindle-cli/console_off.rs` (new), `crates/spindle-cli/tests/spindle-cli/main.rs` — inferred, the new test module and its `mod` line
- **Symbols:** `Surface::with_console`, `Console::ALL`, `serve::Serving`, `Execute` — cited
- **Also likely:** `crates/spindle-ingress/src/surface.rs:1513-1534` — inferred, doc text only; no change needed if the switch lives in the CLI
- **Also likely:** `crates/spindle-embed/src/lib.rs:175` — inferred, only if the Service Worker and Node export get the switch; scoped out (not network listeners)
- **Documents:** `website/docs/reference/cli.md` (generated from `spindle_cli::cli`, regenerate), `CHANGELOG.md` `[Unreleased]`, `README.md` (console paragraph), `AGENTS.md` `spindle-cli` row — inferred
- **Confidence:** high — the story names the routes and the gate; the tree shows one call site that adds them
- **Would collide with:** any unit touching `spindle-cli`'s `run` flags (`cli.rs`, generated `cli.md`), the serve start-up in `serve.rs`/`main.rs`, or the CLI test module list `tests/spindle-cli/main.rs`
- **Safety fact:** a console answer is reachable only through `Resolved::Console`, which `Surface::resolve` returns only while `self.console` is true (`surface.rs:1625-1634`); `console` starts false (`surface.rs:1452`) and only `with_console` sets it (`surface.rs:1531`), so without that call `/api/*` falls through to the routes and answers `NotFound` (404), and the reset path in `serve.rs:495,509` keys on `exchange.console()`, then `None` — step 2, unproven

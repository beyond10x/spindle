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
revision: 1
---
# An exposed run serves no console route unless asked

## Outcome

An operator turns the console routes off with one flag or environment entry: `/api/reset`,
`/api/events`, `/api/model` and `/api/served` then answer `404` like any unrouted path, on every
listener, whatever secret a request carries.

## Why

The console routes are added to every surface whose `ingress.network` is bound to
`http-loopback` (`Surface::with_console`, `crates/spindle-ingress/src/surface.rs:1082-1108`;
`Console::ALL`, `crates/spindle-ingress/src/console.rs:30`). Off loopback they require only the
proxy secret (`crates/spindle-cli/src/cli.rs:254-256`), so any proxy that adds the secret to every
request, as a consumer's does, exposes a reset of the whole store and every published event to the
public.

## Acceptance

- `console_routes_off_answer_404_with_the_secret`: with the console off, `POST /api/reset`,
  `GET /api/events`, `GET /api/model` and `GET /api/served` with the right secret each answer `404`,
  and the store keeps its rows.
- `console_routes_off_leaves_the_published_routes_served`: every route ESS publishes answers as
  before.
- `console_routes_on_by_default_on_loopback`: without the flag a loopback run serves them as
  today.

## Decided when scoped

- Whether off becomes the default off loopback, and whether `/__spindle/` follows the same switch.

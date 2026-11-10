---
format: aep.planning-md/3
id: story:ingress-bind-address
kind: story
status: active
title: The HTTP ingress listens on an address the operator chooses
tags:
- downstream-gap
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: AGENTS.md
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/component.rs
- confidence: inferred
  path: crates/spindle-cli/src/deploy.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-control/src/control.rs
- confidence: inferred
  path: crates/spindle-control/tests/spindle-control/control.rs
- confidence: cited
  path: crates/spindle-docs/src/status.rs
- confidence: cited
  path: crates/spindle-ingress/src/listener.rs
- confidence: cited
  path: website/docs/concepts/capabilities.md
- confidence: cited
  path: website/docs/reference/cli.md
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T22:14:35Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T22:14:35Z", actor: "human:timo", revision: 3}
---
# The HTTP ingress listens on an address the operator chooses

## Outcome

`run` and an instance started by `deploy` listen on an address the operator configures, so a
container port can reach them. Today the listener binds loopback only:
`crates/spindle-ingress/src/listener.rs:233`,
`std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))`. The `Host` guard and the request token
(`bind_guarded`, AGENTS.md) are what protect a loopback listener; a non-loopback address needs the
design to say what protects it instead. Cross-origin answers for a browser on another origin are a
second, separate question, and are out of scope here.

## Acceptance

`run` with the address option set to a non-loopback address of the host answers a request sent to
that address, and without the option it still binds loopback only.

## Scope

Derived 2026-11-10 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-ingress` — cited
- **Files:** `crates/spindle-ingress/src/listener.rs:233` (`Listener::serve` binds `Ipv4Addr::LOCALHOST`) — cited
- **Files:** `crates/spindle-ingress/src/listener.rs:127` (`Guard`; its `Host` check accepts only `127.0.0.1:<port>` / `localhost:<port>`, :143-144) — cited
- **Files:** `crates/spindle-cli/src/cli.rs:174` (`run --port`, "on 127.0.0.1") — cited
- **Files:** `crates/spindle-cli/src/main.rs:477` (`execute` passes `port` into `serve::Serving`; the "would be ignored" guard at 505-512) — cited
- **Files:** `crates/spindle-cli/src/serve.rs:324` (`run` binds with unguarded `Listener::bind`; `Serving.port` at 80) — cited
- **Files:** `crates/spindle-cli/src/component.rs:247` (a deployed instance's ingress: `bail!` on a non-loopback `SPINDLE_SETTING_INGRESS`, then `bind_guarded` at 260) — cited
- **Symbols:** `Listener::bind`, `Listener::bind_guarded`, `Listener::serve`, `Guard`, `serve::Serving::port` — cited
- **Also likely:** `crates/spindle-control/src/control.rs:157` (`free_address` picks `127.0.0.1:0` per instance ingress; `INGRESS_SETTING` at 616) — inferred
- **Also likely:** `crates/spindle-cli/src/deploy.rs:20`, `crates/spindle-cli/src/cli.rs:318` (`deploy` args) — inferred
- **Also likely:** `crates/spindle-control/tests/spindle-control/control.rs:514` (asserts the instance ingress is `LOCALHOST`) — inferred
- **Documents:** `website/docs/reference/cli.md` (generated), `AGENTS.md:36`, `website/docs/concepts/capabilities.md:80`, `crates/spindle-docs/src/status.rs:271` — cited
- **Confidence:** medium — the ingress bind site and the `run` path are cited; the story does not say how `deploy` takes the address
- **Would collide with:** units touching `listener.rs` (`serve`/`Guard`), `run`'s argument plumbing (`cli.rs`, `main.rs`, `serve.rs`), `component.rs`'s ingress, `spindle-control`'s instance-ingress assignment, or the generated CLI reference
- **Safety fact:** every ingress socket is bound at `listener.rs:233`, so the default keeps the no-option path loopback-only; but `run` binds unguarded (`serve.rs:324`: no `Host` or token check), so a non-loopback `run` would serve its whole surface, console routes included, unprotected — unproven
- **Not established:** how `deploy` takes the address; whether the control server (`spindle-control/src/server.rs:63`) is in scope; what protects a non-loopback listener; whether `Listener::bind`'s signature changes (about 15 test call sites); whether a non-loopback bind needs a new `ingress.network` implementation beside `HTTP_LOOPBACK` (`spindle-ingress/src/lib.rs:82`).

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
revision: 3
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

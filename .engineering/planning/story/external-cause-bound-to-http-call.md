---
format: aep.planning-md/3
id: story:external-cause-bound-to-http-call
kind: story
status: implemented
title: An environment binds an external cause to an outbound HTTP call
tags:
- downstream-gap
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: Cargo.lock
- confidence: inferred
  path: Cargo.toml
- confidence: inferred
  path: crates/spindle-capability/src/admission.rs
- confidence: cited
  path: crates/spindle-capability/src/environment.rs
- confidence: inferred
  path: crates/spindle-cli/src/cli.rs
- confidence: inferred
  path: crates/spindle-cli/src/decisions.rs
- confidence: inferred
  path: crates/spindle-cli/src/main.rs
- confidence: inferred
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli
- confidence: inferred
  path: crates/spindle-docs/src/status.rs
- confidence: inferred
  path: crates/spindle-host/src/decisions.rs
- confidence: cited
  path: crates/spindle-kernel/src/decider.rs
- confidence: cited
  path: docs/design/spindle.md
- confidence: inferred
  path: ess/domains/deployment.yaml
- confidence: inferred
  path: website/docs/concepts/environment-and-admission.md
- confidence: inferred
  path: website/docs/reference/cli.md
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T22:14:34Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T22:14:34Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-10T14:42:21Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"test_result":1,"review_outcome":2,"verification":1}}}
---
# An environment binds an external cause to an outbound HTTP call

## Outcome

An outcome decided `external:` can be answered by a named outbound port that the environment binds
to an HTTP call (method, URL, timeout, a secret by reference), as the design table lists:
"an outcome is decided `external:` | `external.<cause>` | a named port: HTTP call, named service, a
test double" (`docs/design/spindle.md:95`). The reply, a timeout or a failure answers the cause, and
the model records the result as the outcome it declares. No secret is written in a specification or
an environment document; the document names a reference.

Today an `external` cause is answered only from standard input, `--decisions`, `--answer-default`
or a world's `decisions`, and in HTTP mode a cause nothing answers gets 501 (`docs/reference/cli.md`
per the downstream facts; not re-read here). No port or adapter exists in `crates/`.

The downstream need: an application whose integrations are called by the runtime instead of by a
worker outside it. Which ESS construct carries the request payload of such a call is not
established; the first step is to read ESS 0.90's `external:` and binding constructs and record the
answer in the design.

## Acceptance

On a neutral fixture, an environment that binds one external cause to an HTTP call against a local
test server makes the command that asks the cause send exactly one request to that server, and the
outcome the reply selects is committed.

## Scope

Derived 2026-11-10 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-capability` (the environment document) and `crates/spindle-cli` (where `run` builds its decider) — inferred; the story names no crate
- **Files:** `crates/spindle-capability/src/environment.rs` — cited: `Environment::parse` (:61) is the only parser of `spindle-environment/1`; it is `deny_unknown_fields` (:25) and `provides` maps a capability to implementation names only (:36), so method, URL, timeout and secret reference have nowhere to go today
- **Files:** `crates/spindle-kernel/src/decider.rs` — cited: `Decider` (:25) and `External` (:10) ask every `external.<command>.<outcome>` cause
- **Files:** `crates/spindle-cli/src/serve.rs` (served decider, :294-327), `crates/spindle-cli/src/decisions.rs`, `crates/spindle-cli/src/main.rs` (line-mode decider, :595), `crates/spindle-cli/src/cli.rs` (`Run` doc :136-139) — inferred
- **Files:** `crates/spindle-cli/tests/spindle-cli/` (new test and a local test server), `crates/spindle-cli/tests/fixtures/` — inferred
- **Symbols:** `Environment`, `Environment::parse`, `Decider`, `External`, `NoExternals`, `Answers::served`, `Answers::over`, `decisions::deciding` — cited
- **Also likely:** `crates/spindle-host/src/decisions.rs` (`Answers`), `crates/spindle-capability/src/admission.rs`, `Cargo.toml`/`Cargo.lock` (no workspace crate but `spindle-control` has an HTTP client), `ess/domains/deployment.yaml` (`Environment.provides` :52-58), `crates/spindle-docs/src/status.rs` (:82, :327) — inferred
- **Documents:** `docs/design/spindle.md` (:95) — cited; `website/docs/concepts/environment-and-admission.md` (:72), `website/docs/reference/cli.md` — inferred
- **Confidence:** medium — the environment document and the decider are placed; which crate holds the HTTP decider and which ESS construct carries the payload are open design questions
- **Would collide with:** units changing `spindle-environment/1` or `Environment`, `run`'s decider wiring (`serve.rs`, `decisions.rs`, `cli.rs`), or `Cargo.lock`
- **Safety fact:** an HTTP-backed answer plugs in as one more `Decider` (`decider.rs:25`), so the kernel's `execute` needs no change; `spindle-host` and `spindle-capability` also compile into the browser worker (`spindle-wasm/Cargo.toml:23-24`, `worker.rs:13,224`), so an HTTP client in either reaches the wasm build — unproven
- **Not established:** the ESS 0.90 construct carrying the request payload; the crate holding the HTTP decider; how a secret reference is resolved; line mode, served mode or both; extend `spindle-environment/1` in place or a new version (which would add every `Environment::parse` caller).

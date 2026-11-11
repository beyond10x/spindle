---
format: aep.planning-md/3
id: story:binding-where-condition
kind: story
status: implemented
title: Bindings with a when.where payload condition are evaluated
relations:
- decomposes: epic:downstream-kernel-constructs
- serves: vision:spindle
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-channel/src/route.rs
- confidence: inferred
  path: crates/spindle-channel/tests
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-10T09:02:54Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-10T09:02:54Z", actor: "human:timo", revision: 5}
- {from: "active", to: "implemented", at: "2026-11-11T00:52:27Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":1}}}
---
# Bindings with a `when.where` payload condition are evaluated

## Outcome

A specification whose event binding declares a payload condition (`when: {event: X, where: …}`)
starts. The channel evaluates the condition on the event's payload before it maps the input, and
skips the binding when the condition is false.

## Why

Reported by a downstream consumer on 2026-11-10, on 0.8.0. `spindle_channel::Route::compile`
refuses any binding with a condition before the run starts
(`crates/spindle-channel/src/route.rs:311-313`, main at f9c99a67): "binding <name>: a `where`
condition is not supported by the channel". ESS admits `when.where` from `ess/22`. The consumer's
workaround moves the condition into the invoked command, which declares a refusal the binding
drops.

## Acceptance

- A specification with a binding `when: {event: X, where: <field> == <value>}` starts, with no
  refusal at start.
- An `X` whose payload matches the condition invokes the target command.
- An `X` whose payload does not match invokes nothing and reports nothing as failed.
- The result for both events matches ESS 0.90.0's interpreter on the same specification.

## Scope

- `crates/spindle-channel/src/route.rs` (`Route::compile`, the refusal at :311-313, and where a
  route maps an event) — cited
- `crates/spindle-channel/tests/` and a neutral fixture — inferred
- `CHANGELOG.md` `[Unreleased]` — inferred

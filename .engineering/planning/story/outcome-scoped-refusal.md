---
format: aep.planning-md/3
id: story:outcome-scoped-refusal
kind: story
status: implemented
title: An unsupported outcome refuses only when it is chosen
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: cited
  path: crates/spindle-cli/tests/fixtures/outcome-scoped-refusal
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/outcome_scoped_refusal.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-ingress/src/answer.rs
- confidence: inferred
  path: crates/spindle-ingress/src/surface.rs
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/lib.rs
- confidence: inferred
  path: crates/spindle-kernel/src/supported.rs
revision: 16
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T13:06:15Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-06T13:06:15Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-07T01:57:23Z", actor: "human:timo", revision: 16, decided_on: {"recorded":{"test_result":1,"review_outcome":1,"verification":1}}}
---
# An unsupported outcome refuses only when it is chosen

## Outcome

A command with one outcome spindle cannot run (for example a payload field it cannot mint) stays
served. Its route answers every other outcome; only a request whose chosen outcome is the
unsupported one answers 501, naming the outcome and what is missing. Today the whole route answers
501 as soon as any outcome is unsupported.

## Acceptance

- `a_route_with_one_unsupported_outcome_serves_its_other_outcomes`: the default outcome answers as
  declared.
- `the_unsupported_outcome_answers_501_when_chosen`: the request that selects it gets 501 with the
  outcome named; nothing is written.
- `admission_lists_the_unsupported_outcome`: `admit` and `run` startup name the outcome, not the
  command, as unsupported.

## Origin

A consumer of `b10x-spindle run`, 2026-11-06: a command lost its route to 501 after its
specification gained one outcome with a generated record field that only an opt-in input selects.

## Scope

Delivered in wave 12 (implementor's confirmation table, commits ed97240, fbeb732, 755c126):

- `crates/spindle-kernel/src/supported.rs` — inferred, confirmed: `selects` (whole command) and `takes` (the taken outcome), `compensates` and `affects:` moved to `takes`.
- `crates/spindle-kernel/src/execute.rs`, `crates/spindle-kernel/src/lib.rs` — not in the inferred scope, needed: `execute_refusing`.
- `crates/spindle-host/src/host.rs` — not in the inferred scope, needed: `Host::submit_refusing`.
- `crates/spindle-ingress/src/surface.rs` — inferred, confirmed: `fn refuse`, `Route::refused`, the Command arm of `served`.
- `crates/spindle-ingress/src/answer.rs` — inferred, confirmed.
- `crates/spindle-cli/src/main.rs`, `serve.rs`, `cli.rs` — not in the inferred scope, needed: the startup lines of `admit`, `run` over HTTP and `run` reading stdin.
- Tests: `crates/spindle-cli/tests/spindle-cli/outcome_scoped_refusal.rs`, `adversary_outcome_scoped_refusal.rs`, fixtures `outcome-scoped-refusal*`, `adversary-outcome-scoped-refusal-*`; `crates/spindle-kernel/tests/spindle-kernel/error_fields.rs` rewritten to the new contract.

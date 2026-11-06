---
format: aep.planning-md/3
id: story:outcome-scoped-refusal
kind: story
status: active
title: An unsupported outcome refuses only when it is chosen
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-ingress/src/answer.rs
- confidence: inferred
  path: crates/spindle-ingress/src/surface.rs
- confidence: inferred
  path: crates/spindle-kernel/src/supported.rs
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T13:06:15Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-06T13:06:15Z", actor: "human:timo", revision: 3}
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

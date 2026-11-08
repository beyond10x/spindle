---
format: aep.planning-md/3
id: story:external-answers-observed
kind: story
status: implemented
title: An observed run records what answered each external cause
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability
- depends_on: story:ingress-request-log
- depends_on: story:ingress-prometheus-metrics
- depends_on: story:ingress-observe-panel
- depends_on: story:fixed-caller-per-grant
scope:
- confidence: cited
  path: crates/spindle-cli/src/decisions.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/observe.rs
- confidence: inferred
  path: crates/spindle-ingress/src/observe.rs
- confidence: cited
  path: crates/spindle-kernel/src/decider.rs
revision: 10
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T14:45:47Z", actor: "human:timo", revision: 8, decided_on: {"recorded":{"review_outcome":1}}}
- {from: "proposed", to: "active", at: "2026-11-06T14:45:47Z", actor: "human:timo", revision: 9, decided_on: {"recorded":{"review_outcome":1}}}
- {from: "active", to: "implemented", at: "2026-11-08T17:25:49Z", actor: "human:timo", revision: 10, decided_on: {"recorded":{"test_result":1,"review_outcome":2,"verification":1}}}
---
# An observed run records what answered each external cause

## Outcome

With `--observe`, every external cause a served command asks about is recorded with what answered
it. `world` is the decision table of the last reset. `file` is `--decisions`. `default` is the
default branch `--answer-default` takes. `unanswered` means nothing answered and the command
answered `501`. The answers appear in the request's `externals`, in the order asked, and as
`spindle_external_causes_total{command,outcome,source}`. The decider `run` builds
(`Answers::served`, `Answers::over`) is wrapped when `--observe` is given, and only then. The
`Decider` trait does not change.

## Acceptance

- `an_external_cause_answered_by_the_decisions_file_is_recorded_as_file`.
- `an_external_cause_answered_by_the_reset_world_is_recorded_as_world`.
- `an_external_cause_answered_by_answer_default_is_recorded_as_default`.
- `an_unanswered_cause_is_recorded_as_unanswered`: the record is `not-served`, its `reason` the
  501 text, its `externals` `[{"outcome", "source": "unanswered"}]`.
- `external_causes_are_counted_per_command_outcome_and_source`: the metric matches the records
  of a scripted sequence.
- `without_observe_the_decider_is_not_wrapped` (unit).

## Scope

- `crates/spindle-cli/src/decisions.rs` — cited: `Answers::decide` knows which source answered;
  it returns that source to the wrapper.
- `crates/spindle-cli/src/serve.rs` — cited: wrapping the decider.
- `crates/spindle-kernel/src/decider.rs` — cited, read only.
- `crates/spindle-ingress/src/observe.rs` — inferred: attaching answers to the current record.
- `crates/spindle-cli/tests/observe.rs` — inferred.

## Order

After `story:ingress-request-log` and `story:ingress-prometheus-metrics`. After
`story:ingress-observe-panel`: both edit `crates/spindle-ingress/src/observe.rs`. After
`story:fixed-caller-per-grant` (implemented in wave 10), which edits `serve.rs`. Before
`story:live-topology`, whose `asks` edges count these answers.

## Origin

Consumer ask (2), "external causes answered (decision table vs default)", 2026-11-06, condensed
(see the epic).

---
format: aep.planning-md/3
id: story:served-run-answers-from-decisions
kind: story
status: implemented
title: A served run answers external causes from a decisions file
relations:
- serves: vision:spindle
- depends_on: story:binding-refusal-policies
scope:
- confidence: inferred
  path: crates/spindle-cli/src/decisions.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/ingress.rs
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-05T22:05:51Z", actor: "agent:b10x-chef-orchestrator", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-05T22:05:51Z", actor: "agent:b10x-chef-orchestrator", revision: 5}
- {from: "active", to: "implemented", at: "2026-11-06T03:30:11Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":2}}}
---
# A served run answers external causes from a decisions file

## Outcome

`run` serving `ingress.network` accepts `--decisions` and `--answer-default`, so a served
specification answers external outcomes from the decisions file instead of leaving them undecided.
Today it is refused with exit 1 (`crates/spindle-cli/src/main.rs`): "--decisions and
--answer-default answer external causes of stdin lines, and run serving ingress.network reads no
stdin".

## Acceptance

- `a_served_command_is_answered_from_the_decisions_file` — a run serving HTTP with `--decisions`:
  a request to a command with an external cause takes the branch the file decides.
- `a_served_command_takes_the_default_with_answer_default` — with `--answer-default`, a command
  with a default branch selects it.
- `a_served_run_without_decisions_answers_as_today` — without either flag, nothing changes.

## Origin

Reported by the downstream session (2026-11-05). Accepted for wave 7 beside
`story:binding-refusal-policies`.

## Scope

- `crates/spindle-cli/src/main.rs` — cited: the refusal in `run`.
- `crates/spindle-cli/src/serve.rs` — cited: `serve`, `Serving`.
- `crates/spindle-cli/src/decisions.rs` — inferred.
- `crates/spindle-cli/tests/ingress.rs` — inferred.

## Depends on

- `story:binding-refusal-policies` — both change what `run` serving `ingress.network` runs: this
  story the CLI's serve path, that one the failure-policy check in `Host::new` the served run
  goes through. Recorded as an edge so the two do not run in parallel; land after it.

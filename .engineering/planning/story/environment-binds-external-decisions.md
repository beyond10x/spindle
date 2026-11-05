---
format: aep.planning-md/3
id: story:environment-binds-external-decisions
kind: story
status: draft
title: The environment binds external causes to a decisions file
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-capability/src/environment.rs
- confidence: inferred
  path: crates/spindle-cli/src/decisions.rs
- confidence: inferred
  path: crates/spindle-cli/src/main.rs
- confidence: inferred
  path: examples/environments/laptop.toml
revision: 3
---
# The environment binds external causes to a decisions file

## Outcome

An environment document can bind `external.*` to a decisions file, and `run` checks the name it
is bound to: a run whose environment binds the decisions implementation reads its answers from the
file the environment names. Today `operator-prompt` is the only binding
(`crates/spindle-capability/src/environment.rs`) and `run` does not check the bound name.

## Acceptance

- `an_environment_binding_external_to_a_decisions_file_answers_from_it` — an environment binding
  `external.*` to the decisions implementation with a file: `run` answers external causes from it.
- `run_refuses_an_external_binding_it_does_not_provide` — an unknown implementation name is
  refused, naming it.

## Origin

Pre-existing. Reported by the downstream session (2026-11-05).

## Scope

- `crates/spindle-capability/src/environment.rs` — cited: the `external.*` binding.
- `crates/spindle-cli/src/main.rs` — inferred: `run`'s admission and binding checks.
- `crates/spindle-cli/src/decisions.rs` — inferred.
- `examples/environments/laptop.toml` — inferred.

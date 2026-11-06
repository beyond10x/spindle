---
format: aep.planning-md/3
id: story:decisions-rules-on-input-predicates
kind: story
status: draft
title: Decision rules match on the command's input
relations:
- serves: vision:spindle
revision: 1
---
# Decision rules match on the command's input

## Outcome

A decisions file rule may name a command plus predicates over its input (for example: this command
when `input.target` equals "R" is refused; any other input takes the default), and a served run
applies the first matching rule per request. Today rules key only on command and outcome
(`crates/spindle-cli/src/decisions.rs`).

## Acceptance

- `a_rule_with_an_input_predicate_decides_only_matching_requests` — two requests to one command,
  one matching the predicate, get the two outcomes.
- A predicate over a field the command's input does not declare is refused at load, naming it.

## Origin

Pre-existing. Requested by a consumer serving a test backend.

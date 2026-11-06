---
format: aep.planning-md/3
id: story:conform-synthesis-seeds
kind: story
status: draft
title: conform passes ESS synthesis seeds
relations:
- serves: vision:spindle
revision: 1
---
# conform passes ESS synthesis seeds

## Outcome

`b10x-spindle conform` takes the ESS synthesis seeds option, so a suite whose scenarios carry
establish steps can be synthesized and run by `conform` itself. Today `crates/spindle-cli/src/conform.rs`
has no such option, and such a suite must be synthesized by `ess` first and replayed.

## Acceptance

- `conform_with_seeds_runs_establish_scenarios` — a neutral fixture with establish steps gives the
  same counts under `conform --synthesis-seeds <file>` as under `ess verify conform synthesize` plus
  `replay`.

## Origin

Pre-existing. Found by the `story:scenario-establish-steps` unit in wave 7.

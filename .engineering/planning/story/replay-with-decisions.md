---
format: aep.planning-md/3
id: story:replay-with-decisions
kind: story
status: draft
title: Replayed scenarios pin external answers from a decisions file
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: inferred
  path: crates/spindle-cli/src/decisions.rs
- confidence: inferred
  path: crates/spindle-cli/src/replay.rs
revision: 3
---
# Replayed scenarios pin external answers from a decisions file

## Outcome

`b10x-spindle run --scenario <file> --decisions <file>` replays the scenario and answers external
causes the scenario does not force from the decisions file. Today clap refuses the combination
(`conflicts_with_all` on `scenario` in `crates/spindle-cli/src/cli.rs`, exit 2), so a replayed
scenario cannot pin external answers.

## Acceptance

- `a_scenario_replays_with_a_decisions_file` — a scenario whose step's external cause it does not
  force: with `--decisions` answering it, the step takes the decided branch; exit 0.
- `a_scenario_s_forcing_wins_over_the_decisions_file` — where both answer, the scenario's forcing
  is used (or the chosen order is stated in the test).

## Origin

Pre-existing. Reported by the downstream session (2026-11-05).

## Scope

- `crates/spindle-cli/src/cli.rs` — cited: the conflict on `scenario`.
- `crates/spindle-cli/src/replay.rs` — inferred: where external causes are answered in replay.
- `crates/spindle-cli/src/decisions.rs` — inferred.

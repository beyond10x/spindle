---
format: aep.planning-md/3
id: story:replay-with-decisions
kind: story
status: active
title: Replayed scenarios pin external answers from a decisions file
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: AGENTS.md
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-cli/src/replay.rs
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/review-scenarios
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/adversary_replay.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/replay.rs
- confidence: cited
  path: crates/spindle-conformance/src/target.rs
- confidence: inferred
  path: crates/spindle-conformance/tests/spindle-conformance/forcing.rs
- confidence: inferred
  path: crates/spindle-docs/src/status.rs
- confidence: inferred
  path: website/data/status.json
- confidence: cited
  path: website/docs/reference/cli.md
- confidence: inferred
  path: website/docs/status.md
revision: 11
transitions:
- {from: "draft", to: "proposed", at: "2026-11-22T18:23:52Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-27T07:28:37Z", actor: "human:operator", revision: 11, decided_on: {"recorded":{"review_outcome":8}}}
---
# Replayed scenarios pin external answers from a decisions file

## Outcome

`b10x-spindle run --scenario <file> --decisions <file>` replays the scenario and answers external
causes the scenario does not force from the decisions file. Today clap refuses the combination
(`conflicts_with_all` on `scenario` in `crates/spindle-cli/src/cli.rs`, exit 2), so a replayed
scenario cannot pin external answers.

## Acceptance

- `a_scenario_replays_with_a_decisions_file`: a scenario with an act on a command with an external
  cause and no `outcome:`; with `--decisions` answering that cause, the act's replay line names the
  decided outcome; exit 0.
- `a_scenario_s_forcing_wins_over_the_decisions_file`: with the same file, an act that names another
  outcome under `outcome:` takes that outcome and holds.
- `a_decisions_file_the_model_refuses_exits_one_before_any_step`: a decisions file naming a command
  the model does not have exits 1 before any step.
- `scenario_refuses_each_flag_of_the_stdin_run` no longer lists `--decisions`.

## Origin

Pre-existing. Reported by the downstream session (2026-11-05).

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-conformance/src/target.rs` (`Forced`, `HostTarget`) and `crates/spindle-cli/src/replay.rs` — cited, `Forced` (`target.rs:115-131`) answers every external cause in a replay, and `Replays::run` (`replay.rs:241-277`) builds the `HostTarget` for each bisected prefix
- **Files:** `crates/spindle-cli/src/cli.rs:392` (drop `"decisions"` from `conflicts_with_all`), `:119-134` and `:259-265` (the `--scenario` and `--decisions` docs) — cited
- **Files:** `crates/spindle-cli/src/main.rs:190-197` (the `scenario: Some(..)` arm takes `decisions` and passes it to `replay::replay`) — cited
- **Files:** `crates/spindle-cli/src/replay.rs:51-57` (`replay` signature), `:100-108` and `:196-206` (`Replays` carries the answers), `:257-258` (each prefix's target gets them) — cited
- **Files:** `crates/spindle-conformance/src/target.rs:28-43`, `:53-69`, `:115-162`, `:188-201`: `Forced` holds `Option<spindle_host::Answers>` beneath the forcing, set by a new builder method on `HostTarget`, so `HostTarget::new` and `HostTarget::clocked` keep their signatures — cited
- **Symbols:** `Forced::decide`, `HostTarget::clocked`, `HostTarget::host`, `Replays`, `replay::replay`, `spindle_host::Answers::decide` (`crates/spindle-host/src/decisions.rs:222`), `crate::decisions::load` — cited
- **Tests:** `crates/spindle-cli/tests/spindle-cli/replay.rs`: `a_scenario_replays_with_a_decisions_file` (decisions `["probe.review.Submit"] flagged = true`, an act on `Submit` with no `outcome:`; its line ends `outcome flagged, error probe.review.Flagged`; exit 0), `a_scenario_s_forcing_wins_over_the_decisions_file` (same file, the act names `outcome: escalated`; it holds; if the file answered first, `flagged` would be taken and the act would diverge), `a_decisions_file_the_model_refuses_exits_one_before_any_step` — cited (file and harness), test contents inferred
- **Tests:** `crates/spindle-cli/tests/spindle-cli/adversary_replay.rs::scenario_refuses_each_flag_of_the_stdin_run` drops the `--decisions` case — cited (`:190-207`)
- **Also likely:** `crates/spindle-conformance/tests/spindle-conformance/forcing.rs`: `a_decided_cause_answers_where_nothing_is_forced`, pins the order in the library — inferred
- **Also likely:** new fixtures `crates/spindle-cli/tests/fixtures/review-scenarios/` (two `ess-scenario/1` documents over `tests/fixtures/review` with `memory.toml`); the decisions file is written to a temporary directory as `tests/spindle-cli/decisions.rs` does — inferred
- **Documents:** `website/docs/reference/cli.md` (generated from the clap docs by `crates/spindle-docs/src/cli.rs:10`) — cited; `crates/spindle-docs/src/status.rs:487-497` "Scenario replay" with `website/data/status.json`, `website/docs/status.md`, the `AGENTS.md` row for `spindle-conformance` ("external: causes answered as a scenario forces them"), `CHANGELOG.md` — inferred
- **Confidence:** high — the story names the conflict site, and the tree shows the single decider every replay uses
- **Would collide with:** `crates/spindle-cli/src/cli.rs` and its generated `website/docs/reference/cli.md` (other stories add or change `run` flags); `crates/spindle-cli/src/main.rs` (the `Command::Run` match); `crates/spindle-conformance/src/target.rs` (`HostTarget` struct and its `ConformanceTarget` impl); the status data in `crates/spindle-docs/src/status.rs` and `website/data/status.json`; `CHANGELOG.md`
- **Safety fact:** without `--decisions`, `Forced` answers exactly as today (forced cause holds; every other cause of the forced command, and every unforced cause, does not hold), so `conform` and the about 40 `HostTarget::new` callers keep ESS parity — step 2 (`target.rs:118-131`, `git grep -n 'HostTarget::\(new\|clocked\)'`), unproven
- **Architecture:** no new crate, process, wire format or external dependency; `Answers` already lives in `spindle-host`, which `spindle-conformance` already depends on — cited
- **depends_on:** none — inferred

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: a cause neither forced nor in the file does not hold, as today and as ESS's
  runner reads it; `--answer-default` stays refused with `--scenario`.
- D2 A: the scenario's forcing first, for every cause of the forced command until
  it is consumed or lapses, then the file.

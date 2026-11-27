---
format: aep.planning-md/3
id: story:one-time-response-admission
kind: story
status: active
title: A scenario with a one-time response is admitted as ESS admits it
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-cli/src/replay.rs
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/one-time-replay
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/one-time-replay-scenarios
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/adversary_responses.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/replay.rs
revision: 13
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T07:28:37Z", actor: "human:operator", revision: 12, decided_on: {"recorded":{"review_outcome":8}}}
- {from: "proposed", to: "active", at: "2026-11-27T07:28:37Z", actor: "human:operator", revision: 13, decided_on: {"recorded":{"review_outcome":8}}}
---
# A scenario with a one-time response is admitted as ESS admits it

## Outcome

A conformance scenario whose external cause carries a one-time response runs under
`b10x-spindle conform` and replay. Today `run --scenario` ends in an error instead of a "diverges"
line when such a scenario does not pass: `Replays::run` (`crates/spindle-cli/src/replay.rs`) bisects
with prefixes that keep the whole scenario's one-time trace, and ESS's `ess-conformance` refuses a
prefix with `InvalidOneTimeResponse` ("one-time event window is not anchored to an operation").
`conform` already runs such a model.

## Acceptance

- `a_divergent_one_time_scenario_names_its_divergent_step`: replaying a one-time scenario whose later
  act diverges exits 1 with a "diverges" line naming that step, and no `InvalidOneTimeResponse` on
  stderr. It fails on 0.22.0: the bisection prefix keeps the whole scenario's one-time trace.
- `a_prefix_keeps_only_its_own_one_time_windows` (unit): every prefix of a compiled one-time
  scenario is admitted by `AdmittedSuite::from_suite`. It fails on 0.22.0, which has no restriction
  of the trace to a prefix.
- Guards, which pass on 0.22.0 and stay green: `a_one_time_response_scenario_runs_on_the_host` (a
  neutral fixture with one external cause and a one-time response replays with exit 0, as under
  ESS's interpreter) and `conform_runs_a_one_time_external_cause` (`conform` on the same fixture;
  every executed scenario passes).

## Origin

Pre-existing. Reported by a downstream consumer replaying its suite during wave 7.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Stale in the body:** the story says the refusal comes from "spindle's call in
  `crates/spindle-conformance/src/target.rs`". That file makes no ESS admission call; its line 150 is
  `spindle_host::admit`, the capability admission. ESS admission is called at
  `crates/spindle-conformance/src/lib.rs:116,123` and `crates/spindle-cli/src/replay.rs:251` — cited
- **Stale in the body:** "refused at admission" under `conform` does not hold at ESS 0.99.1. `produce::attach`
  checks the trace when it attaches it (`ess-conformance/src/one_time_response/produce.rs:104`), and synthesis
  ends with `self_admission::withdraw_inadmissible` (`synthesize.rs:3775`). A one-time model already runs
  under `conform` (`crates/spindle-cli/tests/spindle-cli/adversary_responses.rs:25`) — cited
- **Stale in the body:** the external cause does not trigger the refusal. `ConfigureExternalOutcome` is
  not an operation step, so no window is anchored to it. Any one-time scenario whose whole replay does not
  pass is refused — inferred
- **Stale in the body:** acceptance bullet 2 (an issue on ESS) has nothing to report. ESS's admission is
  right; the fault is a prefix carrying the whole scenario's trace — inferred
- **Defect site:** `Replays::run` in `crates/spindle-cli/src/replay.rs:242-251` builds
  `ConformanceScenario { steps: self.scenario.steps[..end].to_vec(), ..self.scenario.clone() }`, which keeps
  `one_time_response` with windows and origins indexed over the whole scenario. A bisection prefix that ends
  before the last operation is refused by `Trace::validate_windows` ("one-time event window is not anchored
  to an operation", `one_time_response.rs:528`). The error goes up through `from_suite(...)?`, so the replay
  ends in an error instead of a "diverges" line — cited
- **Primary surface:** `crates/spindle-cli`, replay — cited
- **Files:** `crates/spindle-cli/src/replay.rs` (`Replays::run`, a new helper that restricts the trace to a
  prefix, and its `mod tests`) — cited
- **Symbols:** `Replays::run`, `ConformanceScenario::one_time_response`, `ess_conformance::one_time_response::Trace`
  (public fields `origins`, `required_origins`, `events`, `event_windows`) — cited
- **Tests to add:**
  - `a_divergent_one_time_scenario_names_its_divergent_step` in `crates/spindle-cli/tests/spindle-cli/replay.rs`:
    replays a one-time scenario whose later act diverges; exit 1, a "diverges" line, no
    `InvalidOneTimeResponse` on stderr. This is the test that decides whether the story is still missing;
    expected to fail on the tree today — inferred
  - `a_one_time_response_scenario_runs_on_the_host` in the same file: a neutral fixture with one external
    cause and a one-time response replays with exit 0, and `ess_runner_status` gives `Passed` (parity with
    ESS's interpreter) — cited, the name is the story's
  - `conform_runs_a_one_time_external_cause` in `crates/spindle-cli/tests/spindle-cli/adversary_responses.rs`:
    `conform` on the same fixture; every executed scenario passes — inferred
  - `a_prefix_keeps_only_its_own_one_time_windows` in `replay.rs`'s `mod tests`: every prefix of a compiled
    one-time scenario is admitted by `AdmittedSuite::from_suite` — inferred
  - Each test runs the binary or the runner a few times (bisection runs about log2(steps) + 1 replays):
    each well under 60 s, and the crate's test step stays under 120 s. Not measured — inferred
- **Fixtures:** new `crates/spindle-cli/tests/fixtures/one-time-replay/` (a specification with an
  `external:` outcome, `returns: true`, `one_time_response:`) and
  `crates/spindle-cli/tests/fixtures/one-time-replay-scenarios/` (`holds.yaml`, `diverges.yaml`). ESS's
  validation (`ess-domain/src/command/one_time_response.rs:16-84`) does not forbid one-time fields on an
  external outcome — inferred
- **Documents:** none
- **Confidence:** medium. The defect site and the ESS check are read; that the replay fails there was
  traced by reading the code, not run
- **Would collide with:** any unit touching `crates/spindle-cli/src/replay.rs` (`Replays`, `Replays::run`) or
  `crates/spindle-cli/tests/spindle-cli/replay.rs`
- **Safety fact:** `Trace::validate` reads only the trace, `scenario.steps` and `scenario.source`
  (`one_time_response.rs:431-551`). A trace restricted to windows with `after_step < end`, origins invoked
  inside the prefix and required origins selected inside it (no trace when none remains, as `attach` leaves
  none at `produce.rs:51`) passes every check the whole scenario passed — level 3, unproven
- **Architecture:** no change. No new crate, process, wire format or dependency — inferred
- **depends_on:** none — inferred
- **Shared with wave 28:** `story:replay-with-decisions` lists `crates/spindle-cli/src/replay.rs` (its own
  scope, inferred). The two must run one after the other, or as one unit — cited

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: Spindle restricts the one-time trace to each bisection prefix (windows with
  `after_step < end`, origins invoked inside the prefix, no trace when none remains); no ESS change.

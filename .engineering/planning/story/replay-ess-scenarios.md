---
format: aep.planning-md/3
id: story:replay-ess-scenarios
kind: story
status: active
title: Replay ESS scenarios
relations:
- serves: vision:spindle
- decomposes: epic:m0-run-in-process
- depends_on: story:views-with-filters-and-lists
- depends_on: story:mint-creation-identities
scope:
- confidence: inferred
  path: Cargo.toml
- confidence: cited
  path: crates/spindle-cli
- confidence: inferred
  path: crates/spindle-host
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 3}
---
# Replay ESS scenarios

## Outcome

`b10x-spindle run <spec> --environment <env> --scenario <file>` replays an `ess-scenario/1`
timeline: each step a command with the outcome it expects, `$instance` captures from emitted
events, view asserts of the form "the list contains a row with these fields". Exit 0 when every
step and assert holds; otherwise a non-zero exit and the first divergence printed. The scenario is
read through ESS's own crate (the authored-scenario reader in `ess-conformance` at the pinned tag),
never parsed by Spindle (AGENTS.md: ESS is read through its crates).

## Acceptance

Each is a named test in `crates/spindle-cli/tests/`, run on Spindle's own `ess/` or a fixture.

- `a_scenario_whose_steps_all_hold_exits_zero`.
- `a_capture_feeds_a_later_step` — an id minted by a `creates:` outcome, captured as `$instance`,
  is the input of the next step, which succeeds.
- `the_first_divergent_step_is_named` — a step expecting the wrong outcome exits non-zero and
  prints that step's index and both outcomes; no later step runs.
- `a_view_assert_that_does_not_hold_is_a_divergence` — a list assert naming a row the view does
  not hold exits non-zero and names the assert.
- `a_scenario_ess_refuses_is_refused` — a document with an unknown `format` exits non-zero before
  any step runs and prints ESS's own diagnostic naming the format. Which `ess-scenario` versions
  are admitted is ESS's reader's answer at the pinned tag, not this story's.

## Scope

- `crates/spindle-cli` — cited: the `--scenario` flag on `run` (`src/cli.rs`, `src/main.rs`).
- `crates/spindle-host` — inferred: view asserts read through the host's list views
  (`src/view.rs`).
- `Cargo.toml` — inferred: a workspace dependency on `ess-conformance` at the ESS tag, moved with
  the other ESS crates.

Shared, not typed: `website/docs/reference/cli.md`, `crates/spindle-docs/src/status.rs`,
`README.md`, `AGENTS.md`, `Cargo.lock`.

## Order

First of three stories on `crates/spindle-cli/src/cli.rs` and `src/main.rs`: this one, then
`story:run-over-http-ingress`, then `story:rally-runs-as-three-processes`; each later one
`depends_on` the one before. `story:telemetry-transport` does not touch `crates/spindle-cli` and
may run beside this one.

## Depends on

- `story:views-with-filters-and-lists` — implemented.
- `story:mint-creation-identities` — implemented; added by the plan review, because a capture of
  a created instance's id needs the runtime to mint it.

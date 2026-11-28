---
format: aep.planning-md/3
id: story:conform-awaits-system-steps
kind: story
status: draft
title: Conformance runs a scenario that awaits a system step
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-cli/src/replay.rs
- confidence: cited
  path: crates/spindle-conformance/src/target.rs
- confidence: inferred
  path: crates/spindle-conformance/tests/fixtures/await-system
- confidence: inferred
  path: crates/spindle-conformance/tests/spindle-conformance/await_system_steps.rs
- confidence: inferred
  path: crates/spindle-conformance/tests/spindle-conformance/main.rs
- confidence: inferred
  path: docs/design/spindle.md
revision: 3
---
## Outcome

`conform`, `run --scenario` and `spindle_conformance::HostTarget` run a scenario that awaits a
step its system takes itself: a command every grant of which is a `system: true` actor (ess/27 or
later). The target answers ESS's `Target::await_outcome` by issuing that command into the
scenario's host as the actor that grants it, with inputs chosen in the order of ESS's interpreter
(D1 A), where it answers ESS's default `unsupported` ("this target cannot observe, or hold, a step
its system takes itself") and every such scenario is skipped today. A deadline, a tick or an event
binding cannot be such a step: ESS 0.99.2 refuses a binding whose `invoke:` names a system-issued
command (`ess-domain/src/system_issued.rs:19`), Spindle's deadlines and ticks are periodic
bindings (`crates/spindle-timer/src/lib.rs:1-7`), and a binding's invocations are already observed
through `observe_invocations` (wave 29).

## Why

Reported by a consumer against 0.22.0: 50 of the 62 skips of a suite synthesized by ESS 0.99.8 are
await scenarios. `HostTarget` does not implement `await_outcome`; the default of
`ess-conformance` (pinned 0.99.2) refuses it.

## Acceptance

All tests below are in `crates/spindle-conformance/tests/spindle-conformance/await_system_steps.rs`,
default test step, each under the 60 s bound, over the neutral fixture
`crates/spindle-conformance/tests/fixtures/await-system/`: a specification with a command every
grant of which is a `system: true` actor (ess/27 or later), awaited once with `await_within:` and
once without.

- `an_awaited_system_step_is_observed`: the fixture's synthesized await scenarios pass under
  `conform`, and each answer of `HostTarget::await_outcome` equals `Interpreted::for_model`'s on the
  same steps.
- `a_refused_system_step_is_not_issued`: the synthesized `expect_not_issued` scenario for a state
  that refuses the command passes.
- `an_await_answered_not_issued_fails`: against a wrapper target that answers `NotIssued`, the await
  scenario fails with `ESS-CF-AWAIT` (the red side of the first line).
- `conform` over the fixture reports 0 skipped, and `conformance.toml` keeps `skipped = 0` (D5 A).
- The ESS pin stays at 0.99.2: ESS's `crates/verify/ess-conformance/src/target.rs`, which holds
  `AwaitRequest`, is byte-identical from 0.99.2 through 0.99.10.

## Decided when scoped

Taken 2026-11-28 on the scoper's default, each reversible.

- **D1 How the host takes the awaited step.**
  A (default, taken): on `await_outcome`, `HostTarget` issues the command into the scenario's host
  through `Host::submit_as`, as the `system: true` actor that grants it (the first by name if
  several do). Inputs are chosen in ESS's interpreter's order: the identity input is `instance`; a
  stated input is its `compared` value; otherwise an `Optional` is left absent first, a
  `when_related:` input tries the held row identities, and anything else is minted. The answer is
  `Reached` with the input used, the outcome, any error and the events. The events are also pushed
  to `Running::events` under the request's correlation. A forced external outcome is reached
  through the existing `Forced` decider. No candidate reached gives `NotIssued`.
  B: answer only from steps the host took on its own, as the draft Outcome said. Nothing takes
  such a step, so every await fails instead of being skipped.
  C: one attempt with the identity, the stated values and minted values, without trying
  candidates. It differs from the interpreter where an absent Optional or a held row decides the
  branch.
- **D2 Several candidates reach an accepting branch.**
  A (default, taken): the first in order is committed and answered. Where the interpreter would
  answer unsupported because the candidates reach different branches, Spindle passes; that
  difference goes in the ESS-divergence table of `docs/design/spindle.md`.
  B: try every candidate without committing (on a copy of the store) and answer unsupported where
  the branches differ, as the interpreter does. This needs a snapshot of the host's store that
  the tree does not show.
- **D3 The bound (`within`).**
  A (default, taken): a reached step answers `elapsed_ms: Some(0)`. Nothing reached under `within`
  answers `NotIssued { elapsed_ms: Some(within) }` at once, as the interpreter's logical clock
  does, since nothing else will take the step. As the interpreter does, it answers unsupported
  when the model has a periodic binding. With no bound, `NotIssued { elapsed_ms: None }`.
  B: advance the target's clock by `within` when it is a manual clock, else unsupported.
  C: wait real time. Refused: the 60 s test bound.
- **D4 Replay and command counts.**
  A (default, taken): an await does not add to `HostTarget::answered` or `commands`, and
  `replay.rs` is unchanged. `run --scenario` prints no outcome beside an await act.
  B: push an `Answered` for the await, count it, and make `replay.rs` `sends` count
  `AwaitOutcome` steps, so the replay prints the awaited outcome.
- **D5 The skip-count acceptance.**
  A (default, taken): count it on the fixture suite. `conform` over the neutral spec reports 0
  skipped, `conformance.toml` stays `skipped = 0`, and the counts format is unchanged.
  B: add a system actor and a system-issued command to Spindle's own `ess/` model, so the
  repository's own suite has await scenarios.
  C: a separate await skip count in the counts file (`spindle-conformance-counts/2`) printed by
  `check:conformance`.

## Scope

Derived 2026-11-28 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-conformance` — cited (the story names `spindle_conformance::HostTarget`)
- **Files:** `crates/spindle-conformance/src/target.rs` (`HostTarget` struct :61-79, its doc :34-60, `Running` :128-135, `send` :338-519, `impl ConformanceTarget for HostTarget` :522-732, which has no `await_outcome`) — cited
- **Symbols:** `ConformanceTarget::await_outcome`, `AwaitRequest` (`command`, `instance`, `within`, `deadline`, `compared`, `correlation`), `AwaitAnswer::{Reached, NotIssued}`, `HostTarget` — cited (ESS 0.99.2 rev 8b2da1d `crates/verify/ess-conformance/src/target.rs:548-580`, `:1189-1227`)
- **Parity reference:** ESS 0.99.2 `crates/verify/ess-conformance/src/interpret/await_outcome.rs` (`await_system_outcome`, `choices`, `combinations`, `reaches`, `nothing_reached`) and `src/runner/await_outcome.rs` (the verdict table) — cited
- **Also likely:** `Host::submit_as` (`crates/spindle-host/src/host.rs:2083`) as the way in, sent as the `system: true` actor that grants the command (`ResolvedActor::system`, `ResolvedCommand::system_issued`, `await_within`); no change to `spindle-host` expected — inferred
- **Also likely:** new test module `crates/spindle-conformance/tests/spindle-conformance/await_system_steps.rs`, its `mod` line in `tests/spindle-conformance/main.rs`, and a fixture `crates/spindle-conformance/tests/fixtures/await-system/` (a neutral ess/27+ spec with a `system: true` actor) — inferred
- **Also likely:** `crates/spindle-cli/src/replay.rs` `sends` (:443-460) only if D4 B is taken; it counts only `ExecuteCommand` steps as sends, though `steps` (:388-391) already labels `AwaitOutcome` and `ExpectNotIssued` as acts — cited lines, inferred need
- **Also likely:** `crates/spindle-cli/src/conform.rs` unchanged: it builds `HostTarget::new` at :122, and `run --scenario` builds `HostTarget::clocked` at `replay.rs:268`, so both get the method without edits — inferred
- **Documents:** `CHANGELOG.md` [Unreleased]; one row in the ESS-divergence table of `docs/design/spindle.md` (around :420-445) — inferred
- **Not touched:** `Cargo.toml` ESS pin — cited: `git diff 0.99.2 0.99.10 -- crates/verify/ess-conformance/src/target.rs` in the ESS repo is empty
- **Confidence:** high for `target.rs` (the story names the type and the missing method, and the trait default is read); medium for the test and replay files
- **Would collide with:** any unit editing `HostTarget` in `crates/spindle-conformance/src/target.rs` (struct fields, `send`, the trait impl), and any unit adding a module to `crates/spindle-conformance/tests/spindle-conformance/main.rs`
- **Safety fact:** nothing in Spindle issues a system-issued command unless asked, so `HostTarget` holds its system by construction. ESS 0.99.2 refuses a binding whose `invoke:` names a system-issued command (`ess-domain/src/system_issued.rs:19`, `:113-116`), and Spindle's deadlines and ticks are periodic bindings (`crates/spindle-timer/src/lib.rs:1-7`) — step 2, unproven
- **Safety fact (D1 A, D2 A):** a candidate input the model refuses commits nothing to the host's store, so trying candidates in order leaves the scenario's records unchanged (`crates/spindle-kernel/src/execute.rs:82-83`, "A refusal commits nothing"). Whether `Host::submit_as` still runs bindings on a refused outcome's events (`Report::refused`) was not walked — step 2, unproven

## Not covered

- Await scenarios on a browser or Node host.

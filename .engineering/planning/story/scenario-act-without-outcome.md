---
format: aep.planning-md/3
id: story:scenario-act-without-outcome
kind: story
status: draft
title: A scenario act that states no outcome is reported as ESS's runner reports it
relations:
- serves: vision:spindle
revision: 3
---
# A scenario act that states no outcome is reported as ESS's runner reports it

## Outcome

`run --scenario` reports a timeline act that states no expected outcome the way ESS 0.99.1's
conformance runner judges it: where the runner fails a refused or failed act, Spindle prints it as
not holding, with the outcome observed, and exits 1. Today such an act prints as holding even when
the command was refused, so a scenario reads green over a step that did nothing.

Reported 2026-11-26 by a consumer replaying business scenarios. Inferred, not traced: what ESS's
runner asserts for an act without an expected outcome; the story first establishes it from
`ess-conformance` at the pin and states it here.

## Acceptance

- `an_act_without_an_outcome_that_is_refused_is_reported_as_ess_reports_it`: a scenario whose act
  states no outcome and whose command is refused prints the act with the observed outcome and exits
  as ESS's runner exits for the same document.
- `an_act_without_an_outcome_that_runs_holds`: the same act on a command that records still holds.
- Every act's line prints the outcome observed, whether or not the act states one.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli/src/replay.rs` — cited. The story names `run --scenario`. Its lines are printed at `replay.rs:104-110` (all hold) and `replay.rs:130-133` (holds before the divergent step, then `diverges`). Neither line carries the outcome observed.
- **Files:** `crates/spindle-cli/src/replay.rs:100-153`, `crates/spindle-cli/tests/spindle-cli/replay.rs`, `crates/spindle-cli/tests/fixtures/replay-scenarios/` and the spec in `crates/spindle-cli/tests/fixtures/replay/` — cited, the existing replay tests and fixtures (new fixtures for a refused, outcome-less act are inferred).
- **Symbols:** `replay::replay`, `Replays::run`, `Replayed` (`replay.rs:44`, `:173-206`, `:166-171`) — cited.
- **Also likely:** `crates/spindle-conformance/src/target.rs` (`HostTarget::execute_command`, `:216-348`) — inferred. For an act without `outcome:`, ESS's `ScenarioResult` holds no check that names the outcome taken. To print "the outcome observed" for every act, `HostTarget` would have to record each outcome it took, next to the command count it already keeps (`target.rs:243`).
- **Also likely:** `crates/spindle-cli/src/cli.rs:93-99` (the `run --scenario` help text) — inferred.
- **Documents:** the `run --scenario` help text and the "Scenario replay" entry in `crates/spindle-docs/src/status.rs:450-455` — inferred. The story's own Outcome paragraph has to be restated from "ESS runner semantics" below — cited from the story's "Inferred, not traced" line.
- **Not touched:** `ess-conformance` (pinned 0.99.1, `Cargo.toml:18`), `spindle-host`, `crates/spindle-cli/src/conform.rs` — inferred.
- **Confidence:** high for `replay.rs`, because the story names `run --scenario` and it is the only printer. Medium for `target.rs`, because whether the observed outcome has to come from the target depends on a design choice.
- **Would collide with:** any unit touching `spindle-cli`'s `replay.rs` or its replay tests and fixtures. Also any unit touching `HostTarget::execute_command` in `spindle-conformance/src/target.rs`, which `conform` shares — inferred.
- **Safety fact:** the exit code stays the runner's verdict (`replay.rs:104` compares `whole.result.status == Status::Passed`), so adding the observed outcome to the printed line changes stdout only. Any outcome `HostTarget` records must leave the `SemanticCommandResult` it returns unchanged, or `conform` changes too. Level 2, unproven.

ESS runner semantics:
- An act's `outcome:` is optional: `authored.rs:474-476` (`#[serde(default)] pub outcome: Option<String>`).
- With no `outcome:`, the compiler emits no `ExpectOutcome` and no `ExpectNoError`: `expected_outcome` returns at once (`authored.rs:4455-4458`). It also adds no guard claim (`guarded_outcome`, `(None, None) => return`, `authored.rs:3741`) and arms no external answer (`authored.rs:3686-3697`). The act compiles to a bare `ExecuteCommand` (`authored.rs:3466-3475`), plus any `error:`, `events:` or `no_events:` claims it does write.
- Exception: when the act names no outcome and every outcome of the command `returns`, the compiler still emits `ExpectDirectResponse` (`authored.rs:3491`, `:3493-3510`).
- The runner's `execute_command` (`runner.rs:1383-1455`):
  - `Ok(result)` records no check and continues (`:1418-1434`). This includes a refusing outcome carrying an error, and an undeclared refusal.
  - `NotGranted` records the command as executed, with no check, and continues (`:1436-1445`).
  - Any other `TargetError` records `target_failure` and stops (`:1446-1453`). That check is `errored`, or `unsupported` for an unsupported error (`runner.rs:4378-4386`).
- A scenario's status is the worst of its checks, starting from `Passed` (`runner.rs:537-540`).
- So for an act stating no outcome and making no other claim, ESS 0.99.1:
  - does **not** fail it when refused (a declared refusing outcome, an undeclared refusal or not granted): it passes;
  - ends the scenario `error` or `unsupported`, not `failed`, when it fails (the target errors).
- Spindle feeds the runner the same way:
  - a refusal becomes `Ok(took(refusing).with_error(..))` (`spindle-conformance/src/target.rs:309-326`);
  - undeclared becomes `Ok(undeclared())` (`:328`);
  - not granted becomes `Err(not_granted)` (`:329`);
  - anything else becomes `Err(unavailable or unsupported)` (`:330-346`).

Gap confirmed: partly.
- **Verdict:** not a gap. A refused outcome-less act passes in ESS's runner and in Spindle (exit 0), and an act whose command fails already prints `diverges` and exits 1 in both (`replay.rs:104`, `:133-153`). The first acceptance test therefore expects exit 0 with the refusal printed, not exit 1.
- **Printing:** confirmed. `replay.rs:106` and `:131` print `<label>: holds` with no outcome, so a refused step reads as green.
- **Not traced:** a command whose outcomes all `return` gets an `ExpectDirectResponse` claim even without `outcome:`, and whether a refusal then fails it is unverified.

Components-only replay: inferred (medium-low). `--scenario` does not refuse a document for addressing the system as a whole, and it does not replay component by component:
- The document has no system or component key. It names one `domain:` (`authored.rs:396-401`, with `deny_unknown_fields` at `:394-395`), checked only against `ir.domains()` (`authored.rs:2849-2854`). Acts may name any declared command.
- Replay runs every component in one host (`replay.rs:1-2`, `cli.rs:93-94`, `spindle-conformance/src/target.rs:29`).
- The restriction a consumer most likely met is routing:
  - every command goes to the component whose `accepts:` lists it (`spindle-host/src/host.rs:2169-2179`);
  - a command no component accepts errors `no component accepts <command>` (`host.rs:205-206`, raised at `:922`, `:1420`, `:1724`);
  - `HostTarget` turns that into `TargetError::unavailable` (`target.rs:343-346`);
  - the runner turns that into an `errored` check (`runner.rs:1446-1453`);
  - so the act prints `diverges` and replay exits 1.
- A model, or part of one, whose commands no component accepts can therefore not be replayed. Another reading: the consumer meant `play --component` (one component), which has nothing to do with `--scenario`.

## Decided when scoped

- ESS 0.99.1's runner passes a refused act that states no outcome (no `ExpectOutcome` is compiled,
  `authored.rs:4455-4458`), and Spindle's verdict already matches. The exit code stays the runner's
  verdict: a refused act without outcome exits 0. The first acceptance test expects exit 0.
- The gap is the printed line: each act's line carries the outcome observed (or the refusal), so a
  refused step no longer reads `holds` alone. `HostTarget` records the outcome it took beside its
  command count, and the `SemanticCommandResult` it returns is unchanged, so `conform` does not move.

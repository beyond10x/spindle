---
format: aep.planning-md/3
id: story:stdin-run-fires-bindings
kind: story
status: draft
title: A stdin run fires the bindings its commands' events match
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: AGENTS.md
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-cli/Cargo.toml
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/stdin-bindings
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/binding_where.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/stdin_bindings.rs
- confidence: cited
  path: crates/spindle-docs/src/status.rs
- confidence: cited
  path: website/data/status.json
- confidence: cited
  path: website/docs/examples/rally.md
- confidence: cited
  path: website/docs/reference/cli.md
- confidence: cited
  path: website/docs/status.md
revision: 17
---
# A stdin run fires the bindings its commands' events match

## Outcome

A stdin `run` fires the specification's bindings: when a line's command emits an event that a
binding maps to a system-issued command, the bound command runs in the same step, as a served run
runs it, and its answer is printed after the line's own answer. Today a stdin run prints "run
reading stdin carries no binding" (`crates/spindle-cli/src/main.rs:649`) and fires none, so a
specification that wires a reaction as a binding cannot be checked with stdin lines; bindings fire
only when Spindle serves HTTP, which needs a component `reached_by: network`.

## Acceptance

- `a_stdin_line_fires_the_binding_its_event_matches`: a specification with one binding from event
  `E` to a system command `C`; a stdin line whose command emits `E` prints its answer, then `C`'s
  answer, marked as caused by that line, and `C`'s write is visible to the next line.
- `a_bound_command_refused_is_printed_and_does_not_fail_the_line`: `C` answering a declared
  refusing outcome prints that outcome; the run exits 0 (story:run-fails-on-rejected-line counts a
  rejected bound command as a rejection).
- `a_stdin_run_and_a_served_run_fire_the_same_bindings`: one sequence of commands sent on stdin and
  over HTTP leaves the same rows and the same event log.
- `a_specification_without_bindings_prints_as_today`: no extra line.

## Not covered

- Periodic bindings in a stdin run (no `run` mode fires them; story:clock-and-mint-in-every-mode).
- Bindings across processes of a split run.

## Origin

A consumer, 2026-11-26, on Spindle 0.21.0: its gate and a simulation drive Spindle on stdin and
wire reactions as bindings.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli`, the stdin path of `run` in `src/main.rs` — cited
- **Files:** `crates/spindle-cli/src/main.rs:678-684` (`--drop-retries` refused because "run reading stdin carries no binding"; the story's `:649` has moved to `:680`) — cited
- **Files:** `crates/spindle-cli/src/main.rs:738-782` (stdin loop: one `answer` per line, one result line, `rejected` counted from `failure`), `:862-897` (`answer`: one `spindle_kernel::execute_as` on the one store, nothing reads `executed.events`) — cited
- **Files:** `crates/spindle-cli/src/main.rs:532` (`spindle_spec::load`; the stdin path may need `load_with_implied_component`, D3) — inferred
- **Symbols:** `execute` (main.rs), `answer`, `Request`, `spindle_channel::Router::from_ir` / `Router::map` / `Route::invoke` / `Route::answer`, `spindle_host::RETRY_ATTEMPTS`, `spindle_host::DELIVERY_LIMIT` — inferred (reuse the served run's routing over the stdin run's one store)
- **Files:** `crates/spindle-cli/Cargo.toml:52` (`spindle-channel` is a dev-dependency only; D1 A moves it into `[dependencies]`) — cited
- **Files:** `crates/spindle-cli/src/cli.rs:79-99` (stdin `Run` doc: "Each line prints one JSON result"), `:196-198` and `:211-215` (`--drop-retries` "Only where run serves HTTP") — cited
- **Tests:** new module `crates/spindle-cli/tests/spindle-cli/stdin_bindings.rs` (the four named acceptance tests), its `mod` line in `crates/spindle-cli/tests/spindle-cli/main.rs`, a new fixture `crates/spindle-cli/tests/fixtures/stdin-bindings/` (one binding `E` → `C`, `C` with a declared refusing outcome, no `reached_by: network`) — inferred
- **Also likely:** `crates/spindle-cli/tests/spindle-cli/binding_where.rs:5` (module doc says stdin carries no binding; the fixture `binding-where` has no network component and could feed test 1) — cited
- **Not touched:** `crates/spindle-host`, `crates/spindle-channel` (read, not changed, under D1 A), `crates/spindle-cli/src/serve.rs` — inferred
- **Documents:** `crates/spindle-docs/src/status.rs:163-169` (row "Event bindings in process and over TCP") with generated `website/data/status.json` and `website/docs/status.md`; generated `website/docs/reference/cli.md`; `website/docs/examples/rally.md:57-58` ("bindings do not fire"); `AGENTS.md:40` (spindle-cli row); `CHANGELOG.md` `[Unreleased]` — cited (lines read), inferred (that each changes)
- **Confidence:** high for the surface (the story names the site and the stdin loop is one function); medium for where the dispatch lives (D1)
- **Would collide with:** the stdin loop, `Request` and `answer` in `crates/spindle-cli/src/main.rs:44-60,738-897`; the `Run` doc in `crates/spindle-cli/src/cli.rs` and its generated page; `crates/spindle-cli/Cargo.toml`; the CLI test module list `tests/spindle-cli/main.rs`; `status.rs` and its generated pages; `CHANGELOG.md`
- **Safety fact:** a stdin run today executes exactly the line's command (`main.rs:882`, one `execute_as`) and no code reads the events it returns, so dispatch added after `answer` changes output and state only where a binding matches an emitted event; a specification without bindings prints as today — cited, level 2, unproven

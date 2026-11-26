---
format: aep.planning-md/3
id: story:guard-over-absent-optional-input
kind: story
status: implemented
title: A guard over an absent optional input is decided as ESS decides it
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/facts.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/fixtures/absent-optional-guard
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/main.rs
revision: 17
transitions:
- {from: "draft", to: "proposed", at: "2026-11-22T18:23:52Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-26T04:51:42Z", actor: "human:operator", revision: 16, decided_on: {"recorded":{"review_outcome":19}}}
- {from: "active", to: "implemented", at: "2026-11-26T08:12:34Z", actor: "human:operator", revision: 17, decided_on: {"recorded":{"test_result":1,"review_outcome":20}}}
---
# A guard over an absent optional input is decided as ESS decides it

## Outcome

A command whose guard reads an optional input that the request leaves out answers as ESS 0.99.1's
interpreter answers it. Today the served HTTP route answers 501 naming the guard as undecidable,
and no test compares that answer with the interpreter.

Read in the tree: Spindle and the interpreter evaluate the guard with the same
`ess_primitives::predicate::Predicate::evaluate`, an absent optional binds no fact on both sides,
and both turn `Truth::Unknown` into an undecidable refusal. Where the interpreter refuses, the 501
is the intended answer, and this story lands as the tests that prove it; where the two differ, the
host is changed to answer as the interpreter.

## Acceptance

- `a_guard_over_an_absent_optional_input_answers_as_the_interpreter`: a neutral fixture with an
  `Optional` input read by a `when:` guard is run through `ess_conformance::interpret` and through
  the host, with and without the input.
  - With the input, both select the same outcome, and the served route answers that outcome's
    status and body.
  - Without the input, where the interpreter answers undecidable, the kernel refuses with
    `ExecuteError::Undecidable` and the served route answers 501 with a body naming the guard as
    undecidable; where the interpreter selects an outcome, the host selects the same one.
- `a_guard_with_defined_over_an_absent_input_selects_its_outcome`: the same fixture with a second
  command whose guard reads `defined(input.x)` answers a declared outcome without the input,
  in process and over HTTP, as the interpreter does.

## Origin

Pre-existing. Reported by a consumer serving a test backend on main `88c1c5a`.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel` — inferred; the input-guard evaluation lives here
- **Files:** `crates/spindle-kernel/src/execute.rs:846` (`Execution::guard`), where a `when:` guard over the input is evaluated — cited
- **Files:** `crates/spindle-kernel/src/facts.rs:146-183` (`Facts::values`, `Facts::bind`): an absent key or `null` binds no fact, so a comparison over it is `Truth::Unknown` — cited
- **Files:** `crates/spindle-kernel/src/execute.rs:732-737` (`holds`): `Truth::Unknown` becomes `ExecuteError::Undecidable`, which the HTTP route answers 501 — cited
- **Symbols:** `Execution::guard`, `Facts::values`, `ExecuteError::Undecidable` (`crates/spindle-kernel/src/lib.rs:171-172`) — cited
- **Also likely:** a new module in `crates/spindle-kernel/tests/spindle-kernel/` with its `mod` line in `main.rs`, built like `absent_optional.rs` (kernel against `ess_conformance::interpret`), and a neutral fixture under `crates/spindle-kernel/tests/fixtures/` — inferred
- **Also likely:** a new module in `crates/spindle-cli/tests/spindle-cli/` with its `mod` line in `main.rs`, the HTTP half — inferred
- **Also likely:** `CHANGELOG.md` `[Unreleased]` — inferred
- **Confidence:** medium — the guard site was traced in the tree; the change may be tests only, because the behaviour reads as matching ESS
- **ESS 0.99.1:** an absent Optional binds nothing (`ess-conformance/src/input.rs:292`, `:1790-1795`) and `Truth::Unknown` becomes `Undetermined::Undecidable` (`interpret/execute.rs:1208-1214`, `:1856-1862`); Spindle evaluates the guard with the same `ess_primitives::predicate::Predicate::evaluate` and the same presence rules (`facts.rs:162`, `:295-301`) — inferred, walked by reading
- **Would collide with:** any unit touching `spindle-kernel`'s input-guard evaluation (`execute.rs` guard and holds, `facts.rs`), the module lists of the `spindle-kernel` and `spindle-cli` integration-test binaries, or `CHANGELOG.md` `[Unreleased]`

## Not covered

- Answering a declared outcome where the interpreter refuses as undecidable: that diverges from
  ESS. A model reaches a declared outcome with `defined(...)` or `coalesce(...)` in its guard.

## Shared files

- `crates/spindle-kernel/src/execute.rs` (`holds` at `:732-737`, `Execution::guard` at `:846`):
  only if the host differs from the interpreter. `story:identity-mint-types-and-fallback` keeps
  out of `execute.rs`.
- `crates/spindle-kernel/tests/spindle-kernel/main.rs` and `crates/spindle-cli/tests/spindle-cli/main.rs`:
  one `mod` line each, merged by the coordinator.
- `CHANGELOG.md` `[Unreleased]`: one bullet per story of wave 25, merged by the coordinator.

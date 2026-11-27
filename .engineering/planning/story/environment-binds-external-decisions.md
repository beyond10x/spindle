---
format: aep.planning-md/3
id: story:environment-binds-external-decisions
kind: story
status: implemented
title: The environment binds external causes to a decisions file
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-capability/src/environment.rs
- confidence: inferred
  path: crates/spindle-capability/src/lib.rs
- confidence: inferred
  path: crates/spindle-cli/src/calls.rs
- confidence: inferred
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/decisions.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/served_decisions.rs
- confidence: inferred
  path: crates/spindle-docs/src/status.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/adversary_environment.rs
- confidence: inferred
  path: website/data/status.json
- confidence: inferred
  path: website/docs/concepts/environment-and-admission.md
- confidence: inferred
  path: website/docs/reference/cli.md
- confidence: inferred
  path: website/docs/status.md
revision: 15
transitions:
- {from: "draft", to: "proposed", at: "2026-11-22T18:23:53Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-27T07:28:37Z", actor: "human:operator", revision: 13, decided_on: {"recorded":{"review_outcome":8}}}
- {from: "active", to: "implemented", at: "2026-11-27T09:53:22Z", actor: "human:operator", revision: 15, decided_on: {"recorded":{"test_result":1,"review_outcome":9}}}
---
# The environment binds external causes to a decisions file

## Outcome

An environment document can bind `external.*` to a decisions file, and `run` checks the name it
is bound to: a run whose environment binds the decisions implementation reads its answers from the
file the environment names. Today `operator-prompt` is the only binding
(`crates/spindle-capability/src/environment.rs`) and `run` does not check the bound name.

## Acceptance

- `an_environment_binding_external_to_a_decisions_file_answers_from_it`: an environment binding
  `external.*` to `decisions-file` with a `[decisions]` table naming a file: a stdin `run` answers
  external causes from it.
- `a_served_run_answers_from_the_environment_s_decisions_file`: the same on a served run.
- `run_refuses_an_external_binding_it_does_not_provide`: an unknown implementation name exits 1
  before anything runs, naming the capability and the name.
- `the_decisions_flag_wins_over_the_environment_s_file`: with both, `--decisions` answers.
- `a_decisions_table_no_external_key_binds_is_refused` (unit): parsing refuses it.
- `a_decisions_file_binding_without_a_file_exits_1`: an environment binding `external.*` to
  `decisions-file` with no `[decisions]` table and no `--decisions` exits 1 before running, naming
  the capability.

## Origin

Pre-existing. Reported by the downstream session (2026-11-05).

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli` (`run`'s `execute`) — cited: the story's `run` checks; `main.rs:533-590` reads the decisions file, admits and binds
- **Files:** `crates/spindle-cli/src/main.rs:533-560` (read the environment before `decisions::load`; refuse an unprovided `external.` implementation beside the `identity.mint` check at `:549-558`) and `:590` (`decides` also true for the environment's file) — cited
- **Files:** `crates/spindle-capability/src/environment.rs:57-77` (`Environment` gains an optional `[decisions]` table with `file`, as `[seed]`/`SeedFile` at `:74-85`), `:196-216` (`parse` refuses a `[decisions]` table that no `external.` key binds to `decisions-file`, as `HttpUnused`) — cited
- **Files:** `crates/spindle-capability/src/lib.rs` (re-export of the new table type) — inferred, as the `[seed]` change did
- **Files:** `crates/spindle-kernel/tests/spindle-kernel/adversary_environment.rs:66-73` — cited: the only struct literal of `Environment`; it breaks when a field is added
- **Symbols:** `Environment`, `EnvironmentError`, `Calls::bind`, `Calls::answer_every`, `execute`, `decisions::load` (called, signature unchanged) — cited
- **Tests:** `crates/spindle-cli/tests/spindle-cli/decisions.rs` (stdin `run`, existing module): `an_environment_binding_external_to_a_decisions_file_answers_from_it`, `run_refuses_an_external_binding_it_does_not_provide`, `the_decisions_flag_wins_over_the_environment_s_file`; `crates/spindle-cli/tests/spindle-cli/served_decisions.rs`: `a_served_run_answers_from_the_environment_s_decisions_file`; unit tests in `environment.rs`: `a_decisions_table_names_its_file`, `a_decisions_table_no_external_key_binds_is_refused`. Each test starts the binary once, well under 5 s; the 60 s per-test and 120 s step budgets hold — inferred from the existing tests of the same shape
- **Also likely:** `crates/spindle-cli/src/cli.rs:185-192, 260-265` (`--decisions` help: the environment's file, and which wins) — inferred
- **Also likely:** `crates/spindle-cli/src/calls.rs:103-148` if the name check sits in `Calls::bind` instead of `main.rs` — inferred
- **Documents:** `website/docs/concepts/environment-and-admission.md:71` (the `external.*` row) and a new section placed after § A seed submitted at start ends (`:240`), not inside it; `website/docs/reference/cli.md`; `crates/spindle-docs/src/status.rs` with `website/data/status.json` and `website/docs/status.md`; `CHANGELOG.md` — inferred, the files the `[seed]` change (`f9a913dc`) touched
- **Confidence:** medium — the story and the tree name the code site; the name set and where the file is named are open decisions (D1, D2)
- **Would collide with:** any unit that adds a field to `Environment` or touches `environment.rs`; any unit changing `run`'s `execute` in `spindle-cli/src/main.rs`; any unit on `--decisions` help in `spindle-cli/src/cli.rs`; any unit adding a row to `crates/spindle-docs/src/status.rs` / `website/data/status.json`
- **Safety fact:** refusing every `external.` implementation except `operator-prompt`, `decisions-file` and a declared `[http.<name>]` call refuses no environment in the tree: those three names are the only external implementations in examples and fixtures, except `["operator"]` in `spindle-capability/tests/spindle-capability/admission.rs:116`, which runs only admission, never `run` — step 2 (`git grep` over `crates`, `examples`, `website`), unproven

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: `run` provides `operator-prompt`, `decisions-file` and every declared
  `[http.<name>]` for `external.*`; any other name is refused before anything runs, naming the
  capability and the name (changelog line: an environment naming an unknown implementation, which
  `run` accepted, is now refused).
- D2 A: a `[decisions]` table with `file = "<path>"`, relative to where `run`
  starts; `--decisions` wins over it, as `--seed` wins over `[seed]`.
- D3 A: `decisions-file` bound with no file named exits 1 before running.
- D4 A, taken by the coordinator after the adversary pass (2026-11-27): the environment's file
  answers only the causes the environment binds to `decisions-file`; `--decisions` still answers
  every external cause. A `[decisions]` table that no `external.` key binds to `decisions-file` is
  refused when the document is read, and `run --scenario`, `play`, `play --component` and
  `deploy` refuse an environment with one.

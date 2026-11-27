---
format: aep.planning-md/3
id: story:ess-pin-0-99-2
kind: story
status: active
title: The ESS pin moves to the release a consumer's specification requires
relations:
- serves: vision:spindle
- depends_on: story:binding-failure-escalates
- depends_on: story:payload-field-through-conversion
- depends_on: story:value-source-through-two-rows
scope:
- confidence: cited
  path: .cargo/config.toml
- confidence: cited
  path: .github/workflows/ci.yml
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: Cargo.lock
- confidence: cited
  path: Cargo.toml
- confidence: cited
  path: Taskfile.yml
- confidence: inferred
  path: conformance.toml
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/binding_where.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/conform.rs
- confidence: cited
  path: crates/spindle-conformance/src/lib.rs
- confidence: inferred
  path: crates/spindle-conformance/tests/fixtures/input-list-quantifier
- confidence: inferred
  path: crates/spindle-conformance/tests/spindle-conformance/before_row_and_reference_lists.rs
- confidence: inferred
  path: crates/spindle-conformance/tests/spindle-conformance/input_list_quantifier.rs
- confidence: inferred
  path: crates/spindle-conformance/tests/spindle-conformance/main.rs
- confidence: cited
  path: crates/spindle-control/Cargo.toml
- confidence: cited
  path: crates/spindle-observe-api/.ess-output/state.json
- confidence: cited
  path: crates/spindle-observe-api/types-report.json
- confidence: cited
  path: crates/spindle-observe-api/types.rs
- confidence: cited
  path: crates/spindle-telemetry/Cargo.toml
- confidence: cited
  path: ess/ess-inputs.yaml
- confidence: cited
  path: ui/src/api/generated/.ess-output/state.json
- confidence: cited
  path: ui/src/api/generated/types-report.json
- confidence: cited
  path: ui/src/api/generated/types.ts
revision: 33
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T08:24:14Z", actor: "human:operator", revision: 31, decided_on: {"recorded":{"review_outcome":5}}}
- {from: "proposed", to: "active", at: "2026-11-27T08:24:14Z", actor: "human:operator", revision: 32, decided_on: {"recorded":{"review_outcome":5}}}
---
## Outcome

Spindle's ESS pin moves from 0.99.1 to 0.99.2, the release a consumer's specification requires,
with the parity suite and the conformance gate green on the new pin.

## Why

Reported by a consumer on 2026-11-27: its specification requires ESS 0.99.2 while Spindle pins
0.99.1 (`Cargo.toml`). `check` and `admit` pass on 0.22.0; nothing beyond them was run, so whether
0.99.2's changes reach Spindle's runtime is not established.

## Acceptance

- The ESS pin names 0.99.2 in every file that names it (`Cargo.toml`, the two crate manifests with
  `schema-contract`, `ess/ess-inputs.yaml`, `Taskfile.yml`, `.cargo/config.toml`, the CI workflow
  with the archive's checksum), `check:the_ess_pin_is_one_release` passes, the generated observe
  types are regenerated, and the tests of the crates that use ESS (`spindle-kernel`, `spindle-host`,
  `spindle-conformance`, `spindle-cli`, including wave 29's `conform_runs_an_escalating_binding`,
  `conform_runs_an_outcome_with_a_conversion` and `conform_runs_a_value_source_through_two_rows`) and
  `check:conformance` are green
  on it.
- `the_accepting_scenario_of_an_input_list_quantifier_sends_several_elements_and_passes`: a neutral
  fixture with a refusal that quantifies over an input list of a value type with a member
  (`List<shelf.box.Label>`: 0.99.2 moves only `<list>.0.<member>` witnesses, so a `List<String>` keeps
  0.99.1's empty list; amended 2026-11-27 after the unit); the synthesised accepting step
  holds two or more elements and Spindle passes every scenario. Its suite differs from 0.99.1's.
- `a_trimmed_witness_on_the_deciding_element_is_answered_as_ess_answers_it`: the refusal witness
  0.99.2 synthesises is answered as ESS's interpreter answers it.
- `CHANGELOG.md` names the synthesis change of 0.99.2 with these tests, and says that its UI change
  does not reach Spindle.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** the ESS pin, one value in seven files: `Cargo.toml:17-23` (7 crates),
  `crates/spindle-control/Cargo.toml:35` and `crates/spindle-telemetry/Cargo.toml:28`
  (`schema-contract`), `ess/ess-inputs.yaml:2` (`requires: ess 0.99.1`), `Taskfile.yml:4` and
  `.cargo/config.toml:2` (`ESS_TOOLCHAIN`), `.github/workflows/ci.yml:102-103` (`ESS_VERSION`
  and `ESS_SHA256` of the release archive) — cited; the same set the 0.99.1 move touched
  (commit `5359cafc`) plus the files that commit `1234ce3a` added
- **Files:** `Cargo.lock` — cited; only the ESS packages move (`cargo update -p` per crate,
  `ess-transport` follows)
- **Files:** `crates/spindle-observe-api/{types.rs,types-report.json,.ess-output/state.json}` and
  `ui/src/api/generated/{types.ts,types-report.json,.ess-output/state.json}` — cited; their
  header names `generator_version: 0.99.1`, so `check:the_generated_types_have_no_drift` fails
  until they are regenerated with 0.99.2. `ess-gen` source did not change, so only the header
  moves. `ess/generated/jsonschema` did not move at 0.99.1 and is not expected to — inferred
- **Files:** `CHANGELOG.md` (Unreleased) — cited by the acceptance; names the synthesis fix with
  its test and states that the UI fix does not reach Spindle
- **Files:** `conformance.toml` — inferred; the suite synthesised from `ess/` may gain scenarios.
  Candidates: `ess/domains/deployment.yaml:174` (`DeclareEnvironment/no-implementation`, `exists`
  over the input `provides`) and `:258-279` (`DecideAdmission/not-decided`, `forall`/`exists` over
  `requires`, `bindings` and `missing`). Not established without running synthesis
- **Symbols:** `spindle_conformance::synthesize` (`crates/spindle-conformance/src/lib.rs:56-59`,
  calls `ess_conformance::synthesize`) — cited; the runtime path the change reaches
  (`b10x-spindle conform` without `--suite`). The kernel, host and ingress do not reach it:
  0.99.2 changes no file of `ess-compiler`, `ess-domain`, `ess-primitives`, `ess-gen`,
  `ess-http`, `ess-yaml`, `ess-transport`, `schema-contract` or ESS's `interpret` (only their
  version lines) — cited from the ESS diff
- **Tests to add:** `crates/spindle-conformance/tests/spindle-conformance/input_list_quantifier.rs`
  with `mod input_list_quantifier;` in `.../main.rs` and a fixture
  `crates/spindle-conformance/tests/fixtures/input-list-quantifier/`: one command whose refusal is
  `exists` over a `List<String>` input with a `trimmed` body. Tests:
  `the_accepting_scenario_of_an_input_list_quantifier_sends_several_elements_and_passes` (the
  synthesised accepting step holds 2 or more elements; Spindle's target passes every scenario) and
  `a_trimmed_witness_on_the_deciding_element_is_answered_as_ess_answers_it` (the `\v\f`..`\r\n`
  literal, Spindle's answer equal to `ess_conformance::interpret`'s) — inferred. One small
  synthesis per test, expected well under 5 s each, each bounded at 60 s by
  `.config/nextest.toml`; the test step stays within its 120 s budget. Every build and test runs
  through `slot` and ends within the pool's 10-minute job limit
- **Also likely:** existing in-process synthesis tests whose fixtures quantify over an input list,
  e.g. `before_row_and_reference_lists.rs` (fixture `spindle-kernel/tests/fixtures/reference-list`),
  `binding_where.rs:158`, `conform.rs:332-410` — inferred; their suites can gain steps and any
  count they assert can move
- **Documents:** `CHANGELOG.md` only. Comments citing "ESS 0.99.1's interpreter" stay; they name
  the release the rule was read from, as at the 0.99.1 move — cited (commit `5359cafc` body)
- **Not touched:** `scripts/wasm-cache-key.sh:347,435,503,512` and `scripts/adversary_w27_u6.sh:96`
  name `0.99.1` in self-contained stub Taskfiles and mutants. The version is incidental there —
  inferred, unproven
- **Confidence:** high for the pin files and the generated headers (the previous pin move and the
  pin check name them); medium for `conformance.toml` and the existing tests (depends on synthesis
  output not run here)
- **Would collide with:** the observe generated types (`crates/spindle-observe-api`,
  `ui/src/api/generated`) and `ess/domains/observe.yaml`, which wave 28's
  `health-and-graceful-drain`, `seed-readiness` and `metrics-series-bounded` change; `Taskfile.yml`
  (`seed-readiness`, another hunk); the Unreleased section of `CHANGELOG.md` (most stories of both
  waves); `Cargo.lock` (any story that adds a dependency); `conformance.toml` (any story that
  changes `ess/`)
- **Architecture:** no change. An existing external dependency moves one patch release; no new
  crate, process, wire format or dependency
- **Safety fact:** ESS 0.99.2 changes no file of the interpreter, compiler, primitives, HTTP or
  YAML crates, so Spindle's runtime answers and its parity target stay as they were; only
  in-process and CLI synthesis move — step 2 (`git diff --stat 0.99.1 0.99.2` in the ESS
  repository: 26 files, sources only under `crates/ui/*` and
  `crates/verify/ess-conformance/src/synthesize.rs`), unproven

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible.

- D1 A: the pin moves to 0.99.2 exactly, the release the consumer requires; 0.99.3 to
  0.99.7 are later synthesis fixes and wait for a story of their own.
- D2 A: "the tests of the crates that use ESS" are the test steps of `spindle-kernel`,
  `spindle-host`, `spindle-conformance` and `spindle-cli`, run after U1 and U2 merge; a
  whole-workspace run is not started on the shared machine.

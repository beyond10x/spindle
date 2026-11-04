---
format: aep.planning-md/3
id: story:load-and-compile
kind: story
status: implemented
title: Load and compile a specification
relations:
- decomposes: epic:m0-run-in-process
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T18:28:15Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-11-04T18:28:15Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-11-04T18:28:15Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
# Load and compile a specification

## Outcome

`spindle_spec::load(path)` reads every `.yaml`/`.yml` file under a specification directory in
path order and compiles them with `ess-compiler`; a document ESS refuses is refused with ESS's own
diagnostics.

## Acceptance

- `spindles_own_specification_compiles` — `ess/` compiles to system `spindle` from 3 files.
- `a_refused_document_reports_what_ess_said` — an invalid version is refused with the ESS message.
- `an_empty_directory_is_refused_by_name`.

## Scope

`crates/spindle-spec/`.

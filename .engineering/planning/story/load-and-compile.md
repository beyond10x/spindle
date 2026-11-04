---
format: aep.planning-md/3
id: story:load-and-compile
kind: story
status: implemented
title: Load and compile a specification
relations:
- decomposes: epic:m0-run-in-process
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-spec
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T18:28:15Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-11-04T18:28:15Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-11-04T18:28:15Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
# Load and compile a specification

## Outcome

`spindle_spec::load(path)` reads the files a specification directory's `ess-inputs.yaml` lists,
in that order, or, without one, every `.yaml`/`.yml` file under the directory in path order, and
compiles them with `ess-compiler`; a document ESS refuses is refused with ESS's own diagnostics.

Updated 2026-11-04 by a plan review: the first version said "every file in path order" and "from
3 files". Commit `9749007` made `ess-inputs.yaml` the order, and `ess/` now lists five files
(`system.yaml`, `components.yaml`, `domains/deployment.yaml`, `domains/placement.yaml`,
`domains/observe.yaml`), which is what the test below asserts.

## Acceptance

- `spindles_own_specification_compiles` — `ess/` compiles to system `spindle` from the five files
  its `ess-inputs.yaml` lists, in that order.
- `a_refused_document_reports_what_ess_said` — an invalid version is refused with the ESS message.
- `an_empty_directory_is_refused_by_name`.

## Scope

- `crates/spindle-spec/` — cited.

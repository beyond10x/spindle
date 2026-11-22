---
format: aep.planning-md/3
id: story:conform-reuses-supplied-suite
kind: story
status: draft
title: conform runs a supplied suite without synthesising one, on one shared admission
relations:
- serves: vision:spindle
revision: 2
---
# conform runs a supplied suite without synthesising one, on one shared admission

## Outcome

- `conform --suite <file>` (`crates/spindle-cli/src/conform.rs:52`) no longer calls
  `spindle_conformance::synthesize` before reading the file. In the 0.17.0 profile of `task
  check:conformance` that synthesis took 1.38 billion instructions, 62.1 % of the command. The
  refusal report (`refused: …` lines and `<n> refusal(s)`) of the authored scenarios is still
  printed with `--suite`: the authored scenarios are checked by ESS without synthesising the suite,
  or the line says the report is not produced for a supplied suite; the exit code for a refused
  authored scenario stays 1.
- `HostTarget` (`crates/spindle-conformance/src/target.rs:118`) builds the compiled model and its
  admission once and shares them (`Arc`) with every host it opens; each scenario still gets fresh
  stores. In 0.17.0, 79 scenarios opened 80 hosts, each cloning the IR and repeating admission:
  6.18 % of the run.

## Acceptance

- `a_supplied_suite_is_not_synthesised`: with `--suite`, a counter or a test hook shows zero
  synthesis calls; stdout and the exit code of `task check:conformance` are byte-identical to
  0.17.0 except for lines this story names.
- `every_scenario_gets_fresh_stores`: two scenarios in sequence do not see each other's rows.
- `admission_runs_once_per_target`.
- `task check:conformance` holds `conformance.toml` unchanged; its wall time before and after is
  in the report.

## Origin

Spindle 0.17.0 review, 2026-11-22: performance findings 1 and 2.

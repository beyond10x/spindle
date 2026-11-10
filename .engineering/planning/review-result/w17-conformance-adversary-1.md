---
format: aep.planning-md/3
id: review-result:w17-conformance-adversary-1
kind: review-result
status: active
title: W17 conformance integration unit, adversary pass 1
relations:
- reviews: story:placement-refusals-match-target
revision: 2
---
unit: wave 17 integration (conformance gate, no known failures, authored scenarios in every path), tree spindle-w17-conformance at aa119dcc plus my test files (not committed)
verdict: NEEDS-CHANGE
cases: executed 351→359, red 2
origin: introduced 3 / pre-existing 0 / undecided 1
wrote-outside-worktree: 1 directory (the assigned scratch directory, listed in part 6)
needs-coordinator: none

**1. Diff.** `git --no-pager diff --stat`: `crates/spindle-cli/tests/spindle-cli/main.rs`,
`crates/spindle-conformance/tests/spindle-conformance/main.rs`,
`crates/spindle-spec/tests/spindle-spec/main.rs`, +1 each (one `mod` line). New untracked files:
`crates/spindle-conformance/tests/spindle-conformance/adversary_w17_conformance.rs`,
`crates/spindle-cli/tests/spindle-cli/adversary_w17_conform.rs`,
`crates/spindle-spec/tests/spindle-spec/adversary_w17_scenarios.rs`. All are test files.

**2. Cases added.** Each file was run alone before the suite. The first conformance run did not
compile (`HostTarget::new` takes `EssIr` by value). I fixed the call to `loaded.ir.clone()` and ran
it again. The results below are from that second run.
- `adversary_w17_conformance::the_scenario_the_changelog_says_passes_passes`: red. The CHANGELOG
  says `LaunchInstance/outcome/not-admitted` passes. It is not in the suite at all, because synthesis
  refuses it. Red output:
  `CHANGELOG says spindle.placement.LaunchInstance/outcome/not-admitted passes; the run holds it as None, synthesis refused it: ["refusal[ESS-SYNTH-001]: outcome spindle.placement.LaunchInstance/not-admitted has no scenario ... no witness: `environment_id` is `spindle.deployment.EnvironmentId`, which names one row per value ..."]  left: None  right: Some(Passed)`
- `adversary_w17_conformance::a_specification_listing_no_scenario_is_not_refused_for_its_listed_scenarios`:
  red. The specification is one file with a `Binary64` actor attribute and lists no scenario.
  `synthesize` returns `SynthesisError::Authored` anyway. Red output:
  `no scenario is listed, and synthesize says ESS refused the listed scenarios: ["refusal[ESS-AUTHOR-036]: model\n  actor.demo.notes.Writer.attributes.rate: a Binary64 in a position the conformance suite does not carry ..."]`
- `adversary_w17_conformance::the_library_suite_is_the_suite_ess_writes`: green. The test runs
  `ess conform synthesize --path ess --scenarios ess` and compares its output with
  `synthesize(&load("ess")).suite.to_canonical_json()`. The scenario ids are equal, and the bytes are
  equal apart from trailing whitespace.
- `adversary_w17_conformance::a_scenario_that_stops_executing_fails_the_counts_gate`: green on the
  tree. A target answers `Unsupported` for every command of the authored scenario. The executed set
  then names `spindle.placement/authored/launch-not-admitted`, and `drops` reports `skipped grew`.
  Red against the mutant described in part 3: `the counts name the drop: ["executed dropped from 79 to 78"]`.
- `adversary_w17_scenarios::{a_scenario_entry_that_climbs_out_of_the_root_is_refused, an_absolute_scenario_entry_is_refused, an_empty_scenarios_list_reads_no_scenario}`:
  green.
- `adversary_w17_conform::a_listed_scenario_ess_refuses_is_printed_and_exits_1`: green. On a copy of
  `ess/` that lists a scenario naming `spindle.deployment.NoSuchCommand`, `conform` exits 1. It
  prints "ESS refused the listed scenarios" with the file name and the unknown command, and writes no
  report.

**3. Suite runs.** `cargo test --locked --no-fail-fast -p spindle-spec -p spindle-conformance -p spindle-cli`
exits 101. Results: spindle-cli 296 passed and 4 ignored; `b10x-spindle` unit tests 3 passed;
spindle-conformance 44 passed, 2 failed (both mine) and 1 ignored; spindle-spec 14 passed. The
`before` count of 351 is the implementing state's: 295 + 42 + 11, plus the 3 unit tests.

Mutant, run on a scratch copy of the tracked files: `Counts::drops`, `lib.rs:382`, became
`if false && self.skipped > committed.skipped`. In that copy the unit's own harness files were
restored, so my cases were not linked. The unit's tests that match `conform`, `gate` or `counts`
(10 spindle-cli, 10 spindle-conformance) all pass on the mutant, exit 0. With my test file copied in,
my case fails on the mutant, as quoted in part 2.

**4. Findings** (aa119dcc)

1. `CHANGELOG.md:58`: NEEDS-CHANGE, introduced. The CHANGELOG says
   `spindle.placement.LaunchInstance/outcome/not-admitted` "passes". ESS synthesis refuses that
   scenario (ESS-SYNTH-001), so it is not in the suite. The diff also removes it from
   `conformance.toml`'s executed set. The known failure went away because the scenario stopped being
   synthesised, not because it now passes. What reaches it: every reader of the release notes. The
   commit message has this right and the CHANGELOG does not. Fix the wording: the scenario is no
   longer synthesised (ESS-SYNTH-001), and `launch-not-admitted` covers the outcome.
2. `crates/spindle-cli/tests/spindle-cli/conform.rs:452`: NEEDS-CHANGE, introduced. The diff sets
   `skipped` in `conformance.toml` from 15 to 0. That makes the `if skipped > 0` branch dead, and it
   was the only test that drove the `skipped grew` gate. The mutant above shows no test of the unit
   notices when that check is removed. What reaches it: a new synthesised scenario that the target
   cannot carry out. The executed set and the executed total stay the same, so only `skipped` would
   catch it. Fix: always test the skipped gate with a counts file whose `skipped` is below the run's,
   for example a failing spec with an unsupported scenario. My test case covers the
   executed-to-skipped direction.
3. `crates/spindle-cli/tests/spindle-cli/conform.rs:217`: CONFIRMED, undecided.
   `every_skipped_scenario_has_a_reason` loops over the scenarios that end `unsupported`. The run
   skips 0, so the loop body never runs and the test passes vacuously. I did not run the base, so I
   cannot say whether the base run also skipped none. Its committed `skipped = 15` was a ceiling, not
   a measured count. Not fixed, as briefed.
4. `crates/spindle-conformance/src/lib.rs:60`: CONFIRMED, introduced, note. `authored::compile`
   checks the model even when there are no sources. A specification that has a `Binary64` in a
   position the suite does not carry, and lists no scenario, now fails with
   `SynthesisError::Authored` ("ESS refused the listed scenarios: ... ESS-AUTHOR-036: model").
   `synthesize`'s `# Errors` section does not allow that error when nothing is listed. The exit code
   of `conform` is unchanged (1; before, the suite was empty and 0 scenarios executed). Only the
   message changes, and the synthesis refusals are no longer listed. What reaches it:
   `b10x-spindle conform <spec>` on such a specification. No specification or fixture in the
   repository has one. Fix: skip `authored::compile` when `specification.scenarios` is empty, or give
   the `model` refusal its own error variant.

**5. Attacked and not broken.**
- Library suite against the `ess` output: identical in scenario ids and in bytes.
- `scenarios:` entries that use `..` or an absolute path: both refused.
- `scenarios: []`: reads none. `conform` runs the synthesised suite only, as `ess` does for a
  model-only argument. Run as `ess conform synthesize --scenarios`, ESS refuses an empty list; the
  task is unaffected because `ess/` lists one scenario.
- A listed scenario that ESS refuses: `conform` exits 1, prints the refusal and runs nothing.
- Authored-scenario order: `authored::compile` sorts by origin itself, so manifest order and ESS's
  sorted order give the same result.
- The counts and set gate catches a scenario that stops executing.
- The four gate properties hold in both the library and the CLI tests.
- `conformance.toml` holds 79 executed scenarios, matching the CHANGELOG.
- The AGENTS.md conformance rows, release step 1, the README and `website/docs/reference/cli.md`
  match the code and the Taskfile.

**6. Outside the worktree.** Everything is under
`~/.cache/scratch/a9933233-1055-4091-b7dc-91db1e33967b/w17/integ/adv/`:
- `tests.diff`, `ess-suite.json`
- `cases.sh`, `suite.sh`, `mutant.sh`
- `case-spec.log`, `case-conf.log`, `case-cli.log`, `cases.done`, `suite.log`, `mutant.log`,
  `mutant-mine.log`
- `mutant-tree/` (the mutated copy of the tracked files)
- `mutant-target/` (its build directory, several GB)

The tests also write under the worktree's `target/tmp` (`CARGO_TARGET_TMPDIR`).

```findings
- file: CHANGELOG.md
  line: 58
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the CHANGELOG says LaunchInstance/outcome/not-admitted passes, but ESS synthesis refuses it (ESS-SYNTH-001) and it is not in the suite at all
- file: crates/spindle-cli/tests/spindle-cli/conform.rs
  line: 452
  category: mutant
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: with committed skipped now 0 the `if skipped > 0` branch is dead, and disabling the skipped-grew check in Counts::drops leaves every unit test green
- file: crates/spindle-cli/tests/spindle-cli/conform.rs
  line: 217
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: undecided
  message: every_skipped_scenario_has_a_reason iterates zero skipped scenarios and passes vacuously
- file: crates/spindle-conformance/src/lib.rs
  line: 60
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a specification listing no scenario but with an uncarried Binary64 fails as SynthesisError::Authored ("ESS refused the listed scenarios"), which the documented Errors section does not allow
```

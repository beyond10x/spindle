---
format: aep.planning-md/3
id: review-result:w17-u2-adversary-1
kind: review-result
status: active
title: U2 placement refusals, adversary pass 1
relations:
- reviews: story:placement-refusals-match-target
revision: 2
---
unit: story:placement-refusals-match-target, tree spindle-w17-placement at 9421ca79 plus my two test-file changes (not committed)
verdict: CONFIRMED
cases: executed 231→236, red 2
origin: introduced 4 / pre-existing 1 / undecided 0
wrote-outside-worktree: 2 paths (the assigned scratch directory, and this report in spindle-wave17), listed in part 6
needs-coordinator: re-pin `conformance.toml` and `conformance-known-failures.json` (6 `conform::*` CLI tests fail until then, as briefed), and decide whether finding 1 is fixed in the kernel (outside the unit's diff) or in the model

The UI is not built in this tree, so 3 CLI tests fail for a reason unrelated to the unit.

**1. Diff.** `crates/spindle-kernel/tests/spindle-kernel/main.rs` +1; new untracked
`crates/spindle-kernel/tests/spindle-kernel/adversary_launch_admission.rs`. Test files only.

**2. Cases added** (`cargo test -p spindle-kernel --test spindle-kernel adversary_launch_admission::`, exit 101).
`SPINDLE_ADVERSARY_ESS` loads another `ess/` directory, so the same cases run against a mutated
copy in scratch.
- `adversary_an_unknown_deployment_is_refused_before_any_external_cause`: red now. Every external
  is answered `true`, and `PlanInstance` gets a deployment nobody offered. Outcome
  `unknown-deployment`, but the decider was asked first. Red:
  `panicked at …/adversary_launch_admission.rs:316:5: ["external.spindle.placement.PlanInstance.not-admitted"]`.
- `adversary_an_unknown_deployment_is_refused_without_a_decider`: red now. The same request with
  a decider that cannot answer. Red: `left: Err("external.spindle.placement.PlanInstance.not-admitted gave no answer: no decision for external.spindle.placement.PlanInstance.not-admitted")`
  `right: Ok("unknown-deployment")`. Written after the first run of the other four, before any
  suite run.
- `adversary_a_placed_instance_of_a_running_deployment_launches`: green now. Red against the
  scratch mutant `predicate: {not: state == Admitted}` (`mutant-no-running.log`, 3 failed). No
  other test in the tree catches that mutant.
- `adversary_admitted_launches_and_offered_does_not`: green now.
- `adversary_only_a_placed_instance_reads_its_deployment`: green now. Planned, running and exited
  instances of a stopped deployment all answer `wrong-state`.
- Run alone: `test result: FAILED. 3 passed; 2 failed; … 231 filtered out`.

**3. Suite runs** (after the cases existed).
- Kernel binary: `234 passed; 2 failed` (both mine). With my module skipped: `231 passed; 5 filtered out`.
- `spindle-target`: `25 passed; 1 failed`. The failure is `adversary_sampling::an_instance_that_ended_without_a_wait_is_not_sampled`; it passes alone, so it is load-timing.
- `spindle-capability`: 16 passed.
- CLI: `283 passed; 10 failed; 4 ignored`. Six are `conform::*` (re-pin) and three need `ui/dist`. The tenth is `adversary_replay::a_long_scenario_replays_in_time_linear_in_its_steps`, which passes alone (timing).
- `ess specify validate --path ess`: valid, exit 0.
- `ess conform synthesize --path ess --scenarios ess`: `77 scenario(s) (1 authored), 4 refusal(s)`, exit 0.
- The schema drift check exits 0.

Probes outside the tree:
- Interpreter, with four probe scenarios (running deployment, stopped deployment, planned instance of an offered deployment, running instance of a stopped deployment) added to a scratch `ess/`: 81 of 81 pass.
- HostTarget (`b10x-spindle conform ess --suite probe-suite.json`): 81 executed, 81 passed.
- Interpreter with the tree's suite against the mutants `predicate: state == Offered` and `{not: state == Admitted}`: 77 of 77 pass for both.

**4. Findings** (9421ca79)

1. `crates/spindle-kernel/src/execute.rs:543`: CONFIRMED, introduced. `PlanInstance`'s new
   `unknown-deployment` is answered, but only after the kernel's look-ahead has asked the decider
   `PlanInstance.not-admitted`. A decider that gives no answer turns the model's refusal into
   `ExecuteError::Undecided`. That breaks the kernel's own contract ("a request the model already
   turns away never reaches it", `execute.rs:584`, `lib.rs:18`) and the story's "refuses an
   unknown deployment first".
   - Reach: any decider-driven run of `ess/` (`run`/`play`) with no decision for that cause. No
     documented workflow sends `PlanInstance`. HostTarget forces answers, so conformance passes.
   - Fix: resolve `missing_related` for input references before `look_ahead` asks a decider.
2. `ess/scenarios/launch-not-admitted.yaml`: CONFIRMED, introduced. The only conformance witness of
   `LaunchInstance/not-admitted` uses an `Offered` deployment against an `Admitted` one. Neither
   predicate mutant is killed by the suite HostTarget runs. The kernel suite kills `state ==
   Offered` but not `{not: state == Admitted}`. My running-deployment case closes the kernel gap.
   - Fix: add a started and a stopped deployment to the authored scenario (the probes in scratch
     pass on both targets).
3. `ess/domains/placement.yaml:314`: CONFIRMED, introduced. The `target-retired` marker says "As
   `when_related: {via: target_id}` it validates". That form is refused (`ESS-COMMAND-005`
   non_exhaustive_branches), because `PlanInstance` does not refuse an unknown target. With an
   added `exists: false` branch it validates, and then 3 sibling refusals lose their scenarios
   (73 scenarios, 10 refusals). So the marker's conclusion holds and its premise does not.
4. `crates/spindle-target/tests/spindle-target/target.rs:641`: CONFIRMED, introduced. The doc
   says the `match` makes a new variant fail "until it is listed, so no refusal escapes". The
   match arms and the `every` vector are separate lists. A variant listed in the match but not
   the vector, whose `cause()` names no declared variant, passes the class test.
5. `crates/spindle-target/src/lib.rs:6`: CONFIRMED, pre-existing (same text on wave/17; also
   `crates/spindle-control/src/lib.rs:8`). The crate docs list the refusals as retired, admission
   and ceiling, and omit the environment mismatch the story now names.

**5. Attacked and not broken.**
- Refusal order: `place` reports retired, then mismatch-or-refused, then each over-ceiling resource. That matches `RefusalCause` and the `PlaceInstance` outcome order.
- The class test fails on swapped `cause()` names and on a dropped `else`.
- `wrong-state` comes first for planned, running and exited instances on the kernel, the interpreter and HostTarget.
- A stopped deployment gives `not-admitted` on all three.
- The `above-ceiling` marker is honest: `ESS-COMMAND-003` for `subject.`/`request.` roots.
- The `PlanInstance` and `PlaceInstance` `not-admitted` markers are honest: as guards they validate and cost 13 and 19 refusals.
- §5a, getting-started and the admit lists agree with the code and the tests.

**6. Outside the worktree.**
- `~/.cache/scratch/a9933233-1055-4091-b7dc-91db1e33967b/w17/placement/adv/`: `ess/` copies (base, probe, mutants, marker forms), probe scenarios, scripts, suites, logs.
- `~/.local/state/worktree/trees/b10x/spindle/spindle-wave17/.engineering/drafts/w17/w17-u2-adversary-1.md` (this report).

```findings
- file: crates/spindle-kernel/src/execute.rs
  line: 543
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: PlanInstance asks the decider PlanInstance.not-admitted before answering unknown-deployment, so a decider without an answer turns the model's refusal into ExecuteError::Undecided
- file: ess/scenarios/launch-not-admitted.yaml
  category: mutant
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the only conformance witness of LaunchInstance/not-admitted covers Offered against Admitted, so the predicate mutants `state == Offered` and `not state == Admitted` both pass the suite HostTarget runs, and the second passes every test in the tree
- file: ess/domains/placement.yaml
  line: 314
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the target-retired UNMAPPED marker says the when_related form validates, and ESS 0.90 refuses it (ESS-COMMAND-005) until an exists-false branch for the target is added
- file: crates/spindle-target/tests/spindle-target/target.rs
  line: 641
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the exhaustive match does not bind the vector it guards, so a variant added to the match and not to the vector escapes the class test the doc says it cannot
- file: crates/spindle-target/src/lib.rs
  line: 6
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: the crate docs of spindle-target and spindle-control list placement refusals without the environment mismatch the specification now names
```

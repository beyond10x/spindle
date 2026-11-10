---
format: aep.planning-md/3
id: review-result:w17-u2-adversary-2
kind: review-result
status: active
title: U2 placement refusals, adversary pass 2 (kernel look-ahead)
relations:
- reviews: story:placement-refusals-match-target
revision: 2
---
unit: story:placement-refusals-match-target, tree spindle-w17-placement at 6c4aef37 plus my test-file additions (not committed)
verdict: NEEDS-CHANGE
cases: executed 236→246, red 6
origin: introduced 3 / pre-existing 1 / undecided 0
wrote-outside-worktree: 2 paths (the assigned scratch directory, and this report in spindle-wave17), listed in part 6
needs-coordinator: re-pin `conformance.toml` and `conformance-known-failures.json` (6 `conform::*` CLI failures, as briefed). Decide whether finding 2, which is pre-existing, is fixed in this wave or goes to the backlog.

`<before>` = 236 is the count the correction commit reports. A rerun with my two modules skipped gives the same number (part 3).

**1. Diff.** `git --no-pager diff --stat`: `crates/spindle-kernel/tests/spindle-kernel/main.rs | 2 ++` (two `mod` lines). Untracked: `crates/spindle-kernel/tests/spindle-kernel/adversary_look_ahead.rs`, `crates/spindle-kernel/tests/spindle-kernel/adversary_launch_refused.rs` and the fixture `crates/spindle-kernel/tests/fixtures/adversary-look-ahead/` (`system.yaml`, `domains/ahead.yaml`, valid under `ess specify validate`). Test files only.

**2. Cases added.**

`adversary_look_ahead.rs` runs each probe on a fresh store after the same setup, on ESS 0.90's interpreter (`Interpreted`, unforced = `Externals::Withheld`, or forced to one external) and on the kernel, with the decider answering the same way. A second check: where the interpreter gives one answer under every forcing, the kernel must give it with a decider that cannot answer.
- `…_a_taken_refusal_before_an_unknown_guard_beside_a_missing_input_row_agrees`: red now. `LendQuiet` declares `withheld` (external refusal) and then `lent-quietly` (`when: quiet == true`, `quiet: Optional<Boolean>`). It is sent with a missing desk and no `quiet`. Green on base 9421ca79.
- `…_beside_a_missing_stored_row_agrees`: red now. The same over a stored `home` (`ReturnQuiet`). Green on base.
- `adversary_no_decider_is_asked_for_a_request_the_model_answers_alone`, plus three per-command copies (`return_quiet_`, `lend_two_`, `lend_approved_`). I split them out after the first run so one red case does not hide the next. All four red now and red on base, where they list more requests.
- `…_an_external_move_beside_a_missing_input_row_agrees`: green now. `approved` is `external:` + `moves:`.
- `…_two_input_rows_one_missing_agree`: green now. `LendTwo` reads desk and spare through the input, each missing, closed or open.
- `…_every_order_of_exists_false_when_subject_when_related_and_external_agrees`: green now and on base. It tries all 120 declaration orders of `no-desk`, `desk-closed`, `held-back` (`external` + `when_subject`), `withheld` and `fast` (accepting `when_related`). ESS accepts 8 of them (`8 declaration orders of LendMixed compiled`). Each accepted order runs 2 books × 4 desks × 3 forcings, plus the no-answer check.

First run alone (`cargo test -p spindle-kernel --test spindle-kernel adversary_look_ahead::`, exit 101), verbatim:
```
---- adversary_look_ahead::adversary_a_taken_refusal_before_an_unknown_guard_beside_a_missing_stored_row_agrees stdout ----
the kernel answers differently from the interpreter:
  probe.ahead.ReturnQuiet {"book_id":"9a8b7c6d-0000-4000-8000-000000000003"} forced Some("withheld"): interpreter `home-gone`, kernel `<no answer>` (asked [])
  probe.ahead.ReturnQuiet {"book_id":"9a8b7c6d-0000-4000-8000-000000000004"} forced Some("withheld"): interpreter `home-gone`, kernel `<no answer>` (asked [])
---- adversary_look_ahead::adversary_no_decider_is_asked_for_a_request_the_model_answers_alone stdout ----
the kernel asks a decider for a request the model answers alone:
  probe.ahead.LendQuiet {"book_id":"9a8b7c6d-0000-4000-8000-000000000001","desk_id":"5d6e7f80-0000-4000-8000-000000000002","quiet":true}: interpreter `desk-closed` under every forcing, kernel `<no answer>` (asked ["withheld"])
  probe.ahead.LendQuiet {"book_id":"9a8b7c6d-0000-4000-8000-000000000001","desk_id":"5d6e7f80-0000-4000-8000-000000000002","quiet":false}: interpreter `desk-closed` under every forcing, kernel `<no answer>` (asked ["withheld"])
---- adversary_look_ahead::adversary_a_taken_refusal_before_an_unknown_guard_beside_a_missing_input_row_agrees stdout ----
the kernel answers differently from the interpreter:
  probe.ahead.LendQuiet {"book_id":"9a8b7c6d-0000-4000-8000-000000000001","desk_id":"5d6e7f80-0000-4000-8000-0000000000ff"} forced Some("withheld"): interpreter `no-desk`, kernel `<no answer>` (asked [])
  probe.ahead.LendQuiet {"book_id":"9a8b7c6d-0000-4000-8000-000000000002","desk_id":"5d6e7f80-0000-4000-8000-0000000000ff"} forced Some("withheld"): interpreter `no-desk`, kernel `<no answer>` (asked [])
test result: FAILED. 2 passed; 3 failed; 0 ignored; 0 measured; 236 filtered out; finished in 0.23s
```
Second run alone, after the split (exit 101). It adds:
```
  probe.ahead.ReturnQuiet {"book_id":"…05","quiet":true}: interpreter `home-closed` under every forcing, kernel `<no answer>` (asked ["withheld"])
  probe.ahead.LendApproved {"book_id":"…01","desk_id":"…02"}: interpreter `desk-closed` under every forcing, kernel `<no answer>` (asked ["approved"])
  probe.ahead.LendApproved {"book_id":"…02","desk_id":"…ff"}: interpreter `wrong-state` under every forcing, kernel `<no answer>` (asked ["approved"])
  probe.ahead.LendApproved {"book_id":"…02","desk_id":"…02"}: interpreter `wrong-state` under every forcing, kernel `<no answer>` (asked ["approved"])
  probe.ahead.LendApproved {"book_id":"…02","desk_id":"…01"}: interpreter `wrong-state` under every forcing, kernel `<no answer>` (asked ["approved"])
  probe.ahead.LendTwo {"book_id":"…01","desk_id":"…02","spare_id":"…02"}: interpreter `desk-closed` under every forcing, kernel `<no answer>` (asked ["withheld"])
  probe.ahead.LendTwo {"book_id":"…01","desk_id":"…02","spare_id":"…01"}: interpreter `desk-closed` under every forcing, kernel `<no answer>` (asked ["withheld"])
test result: FAILED. 2 passed; 6 failed; 0 ignored; 0 measured; 236 filtered out; finished in 0.31s
```
(Ids shortened here; the full output is in `mine-red-2.log`.)

`adversary_launch_refused.rs`: `adversary_a_placed_instance_of_a_refused_deployment_launches_nothing`. It plans and places an instance while its deployment is `Offered` (both `not-admitted` causes answered `false`, as for the scenario's `of-offered`). It then refuses the deployment (`DecideAdmission` `refused`) and expects `LaunchInstance` to answer `not-admitted`. Green now. It honours `SPINDLE_ADVERSARY_ESS`. Against the scratch mutant `{not: {any: [state == Admitted, state == Running, state == Refused]}}` it is red (exit 101):
```
assertion `left == right` failed: a refused deployment is neither admitted nor running: Executed { outcome: "launched", … }
  left: ("launched", None)
 right: ("not-admitted", Some("spindle.placement.DeploymentNotAdmitted"))
```
All six `adversary_launch_admission` cases pass against that mutant (`refused-mutant-others.log`). So does the interpreter suite: `ess verify conform run --target interpreted` passes 77 of 77, exit 0.

Base run: `git archive 9421ca79` into scratch with these cases copied in (`base.log`). Both unknown-guard cases are green there. The four no-answer cases are red there, listing a superset of the requests that are red now.

**3. Suite runs** (after the cases existed).
- `cargo test -p spindle-kernel --test spindle-kernel`: `test result: FAILED. 240 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out`, exit 101. The 6 are the red cases above.
- The same with `--skip adversary_look_ahead --skip adversary_launch_refused`: `236 passed; 0 failed; … 10 filtered out`, exit 0.
- `cargo test -p spindle-target`: `26 passed`, exit 0.
- CLI and conformance runs: not repeated (re-pin pending, as briefed).

**4. Findings** (6c4aef37)

1. `crates/spindle-kernel/src/execute.rs:1030`: NEEDS-CHANGE, introduced.
   - The `ask: false` walk passes over an external branch and keeps reading after it. If a later guard is `Unknown`, `select_ahead` returns `ExecuteError::Undecidable` (`:1116`), before any decider is asked.
   - ESS, with that external forced, stops at it and answers the missing row (`no-desk` / `home-gone`). Base 9421ca79 asked the decider, stopped at the taken refusal, and agreed with ESS.
   - Both call sites are affected: `select` (input row) and `stored_reference` (stored row).
   - Reach: no model in the tree has the shape. The only command with an exists-false row beside an external is `PlanInstance`, and nothing after its external reads `Unknown`. Every construct used is in the kernel's "What runs" list (`lib.rs:37`): `when:` over an Optional input, and `external:`.
   - Fix: when the no-ask walk meets `Truth::Unknown` after it has passed an external, fall back to the asking walk instead of returning the error.
2. `crates/spindle-kernel/src/execute.rs:1041`: CONFIRMED, pre-existing.
   - With every related row present, the look-ahead still asks the decider for an external declared before the held-state check. It does so even where ESS answers the same under every forcing: a present-related refusal (`desk-closed`, `home-closed`), or `wrong-state` from every branch.
   - A decider with no answer turns the model's refusal into `ExecuteError::Undecided`. That breaks "a request the model already turns away never reaches it" (`execute.rs:593`, `lib.rs:18`).
   - Red on base with more cases; the correction closed only the missing-row, no-held-answer subset.
   - Reach: the test fixtures `adversary-related` (`LendBookChecked`, a shelved book at a closed desk) and this pass's fixture. Nothing in `ess/` or `examples/`.
   - Fix: run the no-ask walk for every look-ahead, not only beside a missing row. Ask only where the possible branches differ in their held-state answer.
3. `crates/spindle-kernel/src/execute.rs:1019`: CONFIRMED, introduced (the doc text).
   - The new doc says the decider is asked "only where its answer decides whether the held state answers".
   - The check at `:1033` asks whenever any possible branch has a held-state answer. That includes the case where every one does: `LendApproved`, a lent book at a missing desk, where `approved` and `lent` both answer `wrong-state`. The decider is asked, and with no answer the kernel returns `Undecided` where ESS answers `wrong-state` under every forcing.
   - Fix: the fix in 2, or a doc that says "wherever any branch it could select has a held-state answer".
4. `ess/scenarios/launch-not-admitted.yaml`: CONFIRMED, introduced.
   - The scenario covers four of the deployment's five states and omits `Refused`. The mutant `{not: {any: [state == Admitted, state == Running, state == Refused]}}` passes the interpreter suite (77 of 77) and every existing kernel case.
   - My `adversary_launch_refused` case kills it.
   - Reach: as for the scenario's own `of-offered` instance, only with both `not-admitted` causes answered `false` for an offered deployment. Real `place` (`spindle-target/src/placement.rs:163`) refuses a refused admission, but the deployment here is refused after placement.
   - Fix: add a deployment refused after its instance is placed to the scenario, or adopt the kernel case.

**5. Attacked and not broken.**
- `PlanInstance` with an unknown deployment answers `unknown-deployment` with no decision on hand. The pass-1 F1 cases are green, with every external answered `true` and with a decider that cannot answer.
- `adversary_related_guards`: all its cases pass in the full kernel run.
- External refusal taken beside a missing input or stored row, with no `Unknown` after it: the kernel agrees with ESS and asks only where the answer differs.
- External move (`approved`) beside a missing, closed and open desk, under both forcings: agrees.
- Two input rows, each missing, closed or open (`LendTwo`): agrees under both forcings. The first missing row in exists-false order answers.
- The 8 declaration orders of exists-false, `when_related` refusal, external `when_subject`, external and accepting `when_related` that ESS accepts: the kernel agrees under every forcing and asks no decider where ESS needs none. Base agrees too.
- No decider is skipped where ESS needs its answer: every agreement case under forcing is green.
- `launch-not-admitted.yaml` kills `state == Offered` and `not state == Admitted` (commit's claim; not re-run). It does not kill the `Refused` mutant (finding 4).
- `one_of_each!`: a variant missing from the list fails to compile. A duplicated arm is `unreachable_patterns`, which `task check` turns into an error (`clippy -D warnings --all-targets`). Only a deliberate or-pattern or wildcard arm can let a variant through without an example.

**6. Outside the worktree.**
- `~/.cache/scratch/a9933233-1055-4091-b7dc-91db1e33967b/w17/placement/adv2/`: scripts (`run-mine.sh`, `run-base.sh`, `run-refused.sh`, `suite.sh`, `mutant-refused.sh`, `replace.py`, `shapes.py`), logs (`mine-red-1.log`, `mine-red-2.log`, `mine.log`, `base.log`, `refused-*.log`, `suite-*.log`, `m-refused.log`, `m-refused.json`), the mutant copy `m-refused/` and the base export `base-tree/` (12 MB). The base build directory `base-target/` (1 GB) was mine, and I deleted it after the base run.
- `~/.local/state/worktree/trees/b10x/spindle/spindle-wave17/.engineering/drafts/w17/w17-u2-adversary-2.md` (this report).

```findings
- file: crates/spindle-kernel/src/execute.rs
  line: 1030
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the no-ask look-ahead reads past an external branch and fails Undecidable on a later Unknown guard, where ESS with that external forced answers the missing row's exists-false branch, on both the input-row and stored-row paths
- file: crates/spindle-kernel/src/execute.rs
  line: 1041
  category: acceptance
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: with every related row present the look-ahead still asks the decider where ESS answers a present-related refusal or wrong-state under every forcing, so a decider without an answer turns a model refusal into Undecided
- file: crates/spindle-kernel/src/execute.rs
  line: 1019
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the doc says the decider is asked only where its answer decides the held-state answer, and the check asks whenever any possible branch has one, including where all give the same wrong-state
- file: ess/scenarios/launch-not-admitted.yaml
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: no scenario or kernel case launches an instance whose deployment was refused after placement, so the predicate mutant that admits Refused passes the interpreter suite and every existing kernel case
```

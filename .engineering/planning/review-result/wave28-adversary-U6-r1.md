---
format: aep.planning-md/3
id: review-result:wave28-adversary-U6-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 28 U6
relations:
- reviews: story:environment-binds-external-decisions
revision: 1
---
```
unit: story:environment-binds-external-decisions, unit/w28-env-decisions; findings cover c1d7f499 (tests committed on top as 6ee3596c)
verdict: CONFIRMED
cases: executed 548→555, red 1 (plus the 3 cases already red on the base)
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 7 paths, listed in part 6
needs-coordinator: none
```

**1. Diff stat (c1d7f499..HEAD).** Both paths are test files.
```
 .../tests/spindle-cli/adversary_w28_u6.rs          | 309 +++++++++++++++++++++
 crates/spindle-cli/tests/spindle-cli/main.rs       |   1 +
 2 files changed, 310 insertions(+)
```

**2. Cases added** in `<local path>`, with a `mod` line in `main.rs`. Every case finishes well under 5 s.

- **`a_cause_bound_to_operator_prompt_is_not_answered_from_the_environment_s_file`: red.** The environment binds `"external.*"` to `operator-prompt` and `external.probe.review.Decide.vetoed` to `decisions-file`, with a `[decisions]` file. The case asserts that Decide is answered from the file and that Submit, still bound to `operator-prompt` with no line answer, no `--decisions` and no `--answer-default`, comes back `undecided` and the run exits 1. Red output from running this case alone, before anything else ran:
  ```
  panicked at crates/spindle-cli/tests/spindle-cli/adversary_w28_u6.rs:115:5:
  assertion `left == right` failed: Submit's causes are bound to operator-prompt, and neither the line, --decisions nor --answer-default answers them: [Object {"command": String("probe.review.Submit"), "error": Object {"error": String("probe.review.Flagged"), "fields": Object {}}, "events": Array [], "outcome": String("flagged")}, Object {"command": String("probe.review.Decide"), ... "outcome": String("approved")}]
    left: Null
   right: "undecided"
  test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 587 filtered out
  ```
- **`the_environment_s_file_is_read_relative_to_where_run_starts`: green, kept.** The unit's own test starts `run` in the environment document's directory, so it passes whether the path is read relative to where `run` starts (D2) or relative to the document. This case puts the document in a subdirectory and a second file beside it with the opposite answer, so it fails if the path is resolved against the document. It was red at first because my fixture left `flagged` unanswered; that was my mistake, not a finding, and the fixture is fixed.
- **`an_unprovided_implementation_on_the_last_external_cause_is_refused`: green, kept.** `Submit.flagged` is bound to `desk-phone`, the last external binding in sort order. The unit's cases only put the unknown name on the first binding, so a change that checks only the first binding would have passed them. This case catches it.
- **`one_cause_bound_to_decisions_file_without_a_file_exits_1_naming_it`: green, kept.** D3 on one exact key: exit 1 naming `external.probe.review.Submit.flagged`.
- **`a_missing_environment_decisions_file_exits_1_before_running_naming_it`: green, kept.** Exit 1, nothing on stdout, and stderr names the file.
- **`a_served_run_refuses_an_unprovided_implementation_before_listening`: green, kept.** D1 on a served run: exit 1, no `listening` line.
- **`a_served_run_bound_to_decisions_file_without_a_file_exits_1_before_listening`: green, kept.** D3 on a served run, even with `--answer-default`.

**3. Suite runs (after the cases existed)**

- `cargo test --locked -p spindle-cli --test spindle-cli --no-fail-fast`: exit 101, wall 43 s.
  `test result: FAILED. 551 passed; 4 failed; 33 ignored; 0 measured; 0 filtered out; finished in 31.06s`
  The 4 failures are my red case plus the three cases already red on the base: `deploy::the_ui_is_served_from_the_binary`, `observe_panel::the_panel_is_one_self_contained_page` and `observe_panel::the_panel_may_be_framed`.
- The same command with `-- --skip adversary_w28_u6`, which gives the `before` count: exit 101.
  `test result: FAILED. 545 passed; 3 failed; 33 ignored; 0 measured; 7 filtered out; finished in 31.11s`
- `cargo fmt -p spindle-cli -- --check`: exit 0, after I fixed the formatting of one line.
- `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings`: `Finished`, no warnings.
- One earlier suite run predates my fixture fix and is superseded: 550 passed, 5 failed.

**4. Findings**

- **Operator-prompt causes are answered from the environment's file.**
  - Location: `crates/spindle-cli/src/main.rs:527`, where `decisions_file` returns the file, and `:640`, where `decisions::load` hands it to every cause.
  - Verdict: CONFIRMED. Origin: introduced (the `[decisions]` table does not exist at the base).
  - What was measured: the red case above. When any one external key binds `decisions-file`, the environment's file is loaded as if it came from `--decisions`, so it also answers causes bound to `operator-prompt`.
  - Why it is wrong: the unit's own documents say otherwise. `environment-and-admission.md` § External causes answered from a decisions file says "answers each cause bound to `decisions-file` from it". The `external.*` row says `operator-prompt` is answered "from the line, `--decisions` or `--answer-default`". The CHANGELOG says "answers those causes from it".
  - What reaches it: binding one exact `external.` key while `"external.*"` stays `operator-prompt`. The unit's own unit test `a_decisions_table_no_external_key_binds_is_refused` builds exactly that document and expects it to parse.
  - Fix, named and not applied: answer only the capabilities bound to `decisions-file` from the environment's file, or change the documents to say the file answers every cause.
  - Severity: warning.
- **Judgement, no test: the parse check is looser than its message.**
  - Location: `crates/spindle-capability/src/environment.rs:250`.
  - Verdict: INFEASIBLE, as I could not show anyone writes this. Origin: introduced.
  - The check accepts a `[decisions]` table when `decisions-file` appears anywhere in an `external.` list. But admission (`admission.rs:48`) binds only the first name, so `"external.*" = ["operator-prompt", "decisions-file"]` with a table parses and `run` never reads the file. That is what the refusal message itself says ("would never be read"), yet no refusal comes. The docs say "lists", which matches the code; the story's acceptance says "binds".
  - Severity: note.

**5. Attacked and not broken**

- `--decisions` wins over the table, and the table's file is not read: the unit's own case covers it.
- `play`, `play --component` and `deploy` refuse a `[decisions]` table: the unit's own case covers it.
- `[http.<name>]` calls stay accepted, because the unknown-name check consults `environment.http`.
- A spec with no external outcome triggers no refusal, because only external bindings are checked.
- `identity.mint` is still checked before the external checks; both exit 1.
- Exit code change: a bad `--decisions` file on an environment that admission refuses now exits 3 instead of 1. This matches cli.md ("3 admission refused (run)"), so it is not drift.
- `run --scenario` ignoring the table was not attacked; it is U7's per the wave page.

**6. Paths written outside the worktree**, all under `<local path>`:
- `red-alone.log`
- `module.log`
- `module2.log`
- `suite.log`
- `suite2.log`
- `suite-before.log`
- `commit-msg.txt`

The directory itself was created by me. The worktree lease `adv-w28-u6` was taken and then released.

**7. Findings block**

```findings
- file: crates/spindle-cli/src/main.rs
  line: 527
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the environment's [decisions] file is loaded like --decisions and answers causes bound to operator-prompt too, while the unit's documents say it answers only causes bound to decisions-file
- file: crates/spindle-capability/src/environment.rs
  line: 250
  category: judgement
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: a [decisions] table is accepted when decisions-file is listed after another implementation, which admission never binds, so the file is silently never read despite the refusal's own "would never be read" wording
```

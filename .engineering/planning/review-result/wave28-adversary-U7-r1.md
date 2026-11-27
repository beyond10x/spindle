---
format: aep.planning-md/3
id: review-result:wave28-adversary-U7-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 28 U7
relations:
- reviews: story:one-time-response-admission
- reviews: story:replay-with-decisions
revision: 1
---
```
unit: story:one-time-response-admission, story:replay-with-decisions; findings cover 01b482bed (unit head), tests committed as 4d04a700 on unit/w28-replay
verdict: CONFIRMED
cases: executed 548→554, red 3 (the 3 red on the base, none of them mine)
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 11 paths, listed in part 6
needs-coordinator: none
```

**1. `git --no-pager diff --stat 01b482bed HEAD`** (every path is a test file or a test fixture):
```
 .../fixtures/adversary-w28-u7/no-origin-first.yaml |  23 ++
 .../fixtures/adversary-w28-u7/stated-default.yaml  |  11 +
 .../fixtures/adversary-w28-u7/two-submissions.yaml |  15 ++
 .../adversary-w28-u7/unselected-origin-first.yaml  |  22 ++
 .../tests/spindle-cli/adversary_w28_u7.rs          | 238 +++++++++++++++++++++
 crates/spindle-cli/tests/spindle-cli/main.rs       |   1 +
```

**2. Cases added** in `<local path>`. All 6 are green on the unit head. Run alone, the module printed `test result: ok. 6 passed; 0 failed; ... 581 filtered out`.
- `a_prefix_that_selects_no_one_time_outcome_carries_no_trace`: the first act calls `IssueKey` but states no outcome, so the first prefix requires no one-time origin. ESS's runner gives the whole scenario `Failed`. The replay exits 1, timeline[0] and timeline[1] hold, timeline[2] diverges, and no `InvalidOneTimeResponse` appears.
- `a_prefix_that_invokes_no_one_time_command_carries_no_trace`: the same check, but the first prefix calls no one-time command at all.
- `a_rule_on_the_input_answers_each_act_by_its_own_input`: a `[[rules]] when = {"input.submission_id" = "s1"}` entry answers s1 as `flagged` and s2 as `submitted`.
- `the_file_answers_an_act_whose_stated_outcome_forces_nothing`: ESS passes the act `outcome: submitted`. With `flagged = true` in the decisions file, the replay takes `flagged` and diverges (exit 1). This is what `run --help` says.
- `no_file_and_an_empty_file_answer_as_ess_does`: `unforced.yaml` with no decisions file prints `outcome submitted` (ESS: Passed). An empty decisions file prints byte-identical stdout.
- `answer_default_stays_refused_with_a_decisions_file`: `--scenario --decisions --answer-default` still exits 2.

**Mutant proof** (on a copy of the head in scratch; the tree was never mutated):
- Mutant A, `(!required_origins.is_empty()).then(|| Trace {` changed to `Some(Trace {`. Both one-time cases go red:
  `no prefix is refused: b10x-spindle: admitting .../unselected-origin-first.yaml: $suite: InvalidOneTimeResponse: one-time trace must require an observed origin;`
- Mutant B, the `selected.contains(required)` filter dropped. `a_prefix_that_selects_no_one_time_outcome_carries_no_trace` goes red:
  `"timeline[0] vault.key.IssueKey: holds, outcome held, error vault.key.Held\n" in timeline[0] vault.key.IssueKey: diverges, outcome held, error vault.key.Held / failed ESS-CF-DISCLOSURE`
- The unit's own cases stay green under both mutants:
  - `replay::tests::a_prefix_keeps_only_its_own_one_time_windows ... ok` (under A and under B)
  - `a_divergent_one_time_scenario_names_its_divergent_step`, `a_one_time_response_scenario_runs_on_the_host`, `conform_runs_a_one_time_external_cause` and `a_one_time_field_appears_nowhere_else` all `ok`

**3. Suite run** (after the cases existed): `cargo test --locked -p spindle-cli --test spindle-cli --no-fail-fast`
- Exit 101, wall time 43 s.
- `test result: FAILED. 551 passed; 3 failed; 33 ignored; 0 measured; 0 filtered out; finished in 30.57s`
- The 3 failures are the known base reds (`deploy::the_ui_is_served_from_the_binary`, `observe_panel::the_panel_may_be_framed`, `observe_panel::the_panel_is_one_self_contained_page`).
- "Before" is 548: the same run with my 6 cases left out.
- `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings` exit 0. `cargo fmt -p spindle-cli -- --check` exit 0 after one reflow of my own file.
- No added case is over 5 s.

**4. Findings**
- `crates/spindle-cli/src/replay.rs:343` and `:347` (mutant, warning, CONFIRMED, introduced). Two branches of `prefix` have no test in the unit: no trace when no required origin is left, and a required origin kept only when the prefix selects it. Every prefix of `holds.yaml` and `diverges.yaml` selects `issued` at timeline[0]. The case that reaches it is a one-time scenario whose first act states no outcome or calls another command, replayed with `run --scenario`. The code is correct; the gap is closed by the committed cases.
- `crates/spindle-cli/src/replay.rs:308` (judgement, note, CONFIRMED, introduced). The doc comment says the prefix keeps the windows "as ESS would attach them to these steps". In fact the prefix keeps the whole scenario's `within_ms`, while ESS's `attach` on the prefix would take the prefix's own longest wait. ESS admits both. Keeping the whole scenario's value is the right choice for bisection; only the comment needs to change. Nothing I found reaches a difference on the host.
- `crates/spindle-cli/src/cli.rs:134` (judgement, note, CONFIRMED, introduced). An act that states a non-external outcome (`outcome: submitted`) forces nothing in ESS, so the decisions file overrides what the act states and the act diverges. This matches D2 and the help text, and the kept case pins it. A consumer could still expect a stated outcome to win; that is a design question for the coordinator, not a defect.

**5. Attacked and not broken**
- ESS 0.99.1 `Trace::validate` and `validate_windows`, plus the compact round trip (`expand_trace` must equal the trace). For every prefix the restriction gives what `produce::attach` gives, except `within_ms`.
- Without `--decisions`, `Forced::decide` behaves the same as the base in every branch: forced on the same command, forced on another command, nothing forced, consumed only when it holds.
- Forcing first, then the file: the forced command's other causes return "does not hold" before the file is asked.
- `answers.decide(&BTreeMap::new(), ...)` still matches rules on the input, because the empty map holds the request's own answers, not the input.

**6. Paths written outside the worktree**, all under `<local path>`: `mutants.sh`, `replay.rs.orig`, `commit-msg.txt`, `own.log`, `mutant-always-some.log`, `mutant-unselected-required.log`, `unit-A.log`, `unit-B.log`, `suite.log`, `fmt.log`, `clippy.log`. The mutant copy `mutant-tree/` was made there and removed.

```findings
- file: crates/spindle-cli/src/replay.rs
  line: 343
  category: mutant
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the unit's suite stays green when prefix keeps a trace with no required origin or keeps a required origin the prefix does not select; every fixture prefix selects issued at timeline[0]
- file: crates/spindle-cli/src/replay.rs
  line: 308
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the doc comment says the prefix keeps windows as ESS would attach them to these steps, but it keeps the whole scenario's within_ms where attach on the prefix would take the prefix's own longest wait
- file: crates/spindle-cli/src/cli.rs
  line: 134
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: an act stating a non-external outcome forces nothing, so the decisions file overrides it and the act diverges where ESS passes it; this is consistent with D2 but may surprise a consumer
```

---
format: aep.planning-md/3
id: story:run-fails-on-rejected-line
kind: story
status: active
title: A run that rejects an input line or a seed entry exits non-zero
relations:
- serves: vision:spindle
revision: 9
transitions:
- {from: "draft", to: "proposed", at: "2026-11-26T18:58:24Z", actor: "human:operator", revision: 8, decided_on: {"recorded":{"review_outcome":14}}}
- {from: "proposed", to: "active", at: "2026-11-26T18:58:24Z", actor: "human:operator", revision: 9, decided_on: {"recorded":{"review_outcome":14}}}
---
# A run that rejects an input line or a seed entry exits non-zero

## Outcome

`run` reading JSON lines on stdin, and `run --seed` before it serves, exit with a non-zero status
when a line or a seed entry was rejected, and name the line. Today a rejected line prints its
failure and the run goes on to exit 0, so a script or simulation that drives Spindle reads success
where some of its input never applied.

Rejected means the line or entry did not run: it is not JSON, names no declared command, its input
is not admitted, its external cause is unanswered, or the kernel refused it before an outcome. A
line answered by a declared outcome, a refusing one included, is not rejected. A seed entry counted
`already-there` is not rejected.

Named 2026-11-26 in the operator's order for this wave, and in a consumer's message of that day
listing gaps on Spindle 0.21.0; the consumer's written report does not hold it. That report's
`--scenario` exit-code item is covered by story:scenario-act-without-outcome, where the exit stays
the verdict of ESS's runner.

## Acceptance

- `a_stdin_run_with_a_rejected_line_exits_1`: lines 1 and 3 run, line 2 names an undeclared command;
  all three print their result, and the run exits 1 after the last line, naming line 2 on stderr.
- `a_declared_refusal_is_not_a_rejection`: a line answered by a declared refusing outcome leaves the
  exit status 0.
- `a_seed_with_a_rejected_entry_stops_the_run`: a seed whose second entry is not admitted exits 1
  before listening, naming the entry, and serves nothing.
- `a_seed_entry_rejected_at_run_time_exits_1`: a seed entry admitted at load whose execution fails
  with a host error is counted `rejected` in the seed's end line, apart from declared refusals
  (`refused`); the run prints it on stderr and exits 1 after the answers in hand are written.
- `a_seed_entry_answered_by_a_declared_refusal_keeps_serving`: the end line counts it `refused`,
  and the run keeps serving.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli`: the stdin loop in `src/main.rs` (`execute`), the seed in `src/seed.rs` and `src/serve.rs` — cited
- **Files:** `crates/spindle-cli/src/main.rs:696-716` (stdin loop; line 716 returns `ExitCode::SUCCESS` whatever the lines did), `:740-779` (`answer`: `Ok` carries the outcome and any declared error, `Err` gives `"failure"`) — cited
- **Files:** `crates/spindle-cli/src/seed.rs:83-170` (`load` and `checked` already refuse a malformed, undeclared, unadmitted or bad-caller entry before listening; `main.rs:570-572` exits 1), `:311-344` (`Seeding::step` logs a run-time refusal and goes on), `:366-396` (`submitted` puts a declared refusing outcome, `HostError::Execute` and any other host error into one `Became::Refused`), `:289-306` (end line) — cited
- **Files:** `crates/spindle-cli/src/serve.rs:400-442`, `:477-493` (seed stepped beside the listener; exit only 130/143 on a signal) — cited
- **Files:** `crates/spindle-cli/src/cli.rs:78-98`, `:300-319` (`run` and `--seed` docs say a failing line or refused entry lets the run go on) — cited
- **Symbols:** `execute`, `answer`, `seed::load`, `checked`, `Seeding::step`, `Seeding::end`, `Seeding::finish`, `submitted`, `Became::Refused`, `SeedCounts` (spindle-ingress) — cited
- **Also likely:** `crates/spindle-cli/tests/spindle-cli/run.rs:53,68` (asserts exit 0 after an undeclared command, changes); `seed.rs:455`, `:520`; other CLI tests whose stdin lines fail and assert exit 0 (`decisions.rs`, `http_cause.rs`, `tls_cause.rs`, `caller.rs`, `outcome_scoped_refusal.rs`, `observe.rs`) — inferred
- **Documents:** `README.md:56-64`, `website/docs/concepts/environment-and-admission.md:202-242`, `website/docs/concepts/capabilities.md:70`, `crates/spindle-docs/src/status.rs:430-435` and its generated pages, the generated `website/docs/reference/cli.md`, `CHANGELOG.md` — cited
- **Not touched:** `replay.rs`, `spindle-kernel`, `spindle-host` — inferred
- **Confidence:** high for stdin; medium for the seed (decided below)
- **Would collide with:** `main.rs` `answer` (story:clock-and-mint-in-every-mode), `seed.rs` and the seed start in `serve.rs` (story:seed-once-on-durable-state), the `Run` doc in `cli.rs` and its generated page
- **Safety fact:** a declared refusing outcome comes back as `Ok(executed)` (main.rs:766-773), never in the `failure` branch, so "the result has `failure`" separates a rejection from a declared refusal — cited, level 2
- **Gap confirmed:** stdin yes (main.rs:716, asserted by run.rs:68); seed partly: load-time rejections already exit 1 before listening, run-time ones are counted `refused` beside declared refusals and the run keeps serving — cited

## Decided when scoped

- Stdin: a line whose result carries `failure` is rejected; the run answers every line, then exits 1
  and names the rejected lines on stderr. A declared refusing outcome is not a rejection.
- Seed: an entry rejected at load already exits 1 before listening and stays so. At run time
  `submitted` splits `Became::Refused` into a declared refusing outcome (counted `refused`, as today)
  and a rejection (`HostError::Execute` or any other host error, counted `rejected`, a new count in
  the end line). When the seed ends with a rejection, the run prints it on stderr and exits 1, the
  listener closed after the answers in hand are written. The seed keeps running beside the listener.

## Shared files

- `crates/spindle-cli/src/main.rs`, `seed.rs`, `serve.rs`: shared with
  `story:clock-and-mint-in-every-mode` in the same unit (U3), built before it on one branch.
- The `run` help in `crates/spindle-cli/src/cli.rs` and the generated `website/docs/reference/cli.md`:
  shared with U4 (`--scenario` help); separate hunks, merged by the coordinator, the page generated
  again after both.
- `crates/spindle-docs/src/status.rs:430-435` and its generated pages: shared with U4
  (`:450-455`); separate rows.
- `CHANGELOG.md` `[Unreleased]`: one bullet per story, merged by the coordinator.

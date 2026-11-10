---
format: aep.planning-md/3
id: review-result:w18-u1-adversary-1
kind: review-result
status: active
title: Wave 18 U1 adversary pass 1
relations:
- reviews: story:kernel-guard-reads-now
revision: 1
---
approve

Nothing found: no case I wrote fails against c70c93ed. "approve" here only means that; it does not say the change is correct.

```
unit: wave 18 U1 story:kernel-guard-reads-now, commit c70c93ed (base f9c99a67), worktree spindle-w18-guard-now plus my uncommitted test files
verdict: nothing found
cases: executed 254→264, red 0
origin: introduced 0 / pre-existing 0 / undecided 0
needs-coordinator: none
```

Cases added, in `crates/spindle-kernel/tests/spindle-kernel/adversary_guard_now.rs` on the fixture `crates/spindle-kernel/tests/fixtures/adversary-guard-now/`, each comparing the kernel with the interpreter at one fixed `DecisionInstant`, step by step, and the task rows:

- `a_now_guard_over_a_timestamp_newtype_agrees_with_the_interpreter`: `when:` and `when_subject:` over a newtype of Timestamp, before, at and after now
- `a_now_guard_over_an_absent_timestamp_agrees_with_the_interpreter`: Optional Timestamp absent, null and present, in the input and in a stored row
- `now_plus_a_duration_and_now_on_the_left_agree_with_the_interpreter`: `due_at > now + 24h` at ±1 s around the bound, and `due_at <= now` at its bound
- `a_fractional_instant_and_offset_spellings_agree_with_the_interpreter`: clock at `.5` s, inputs at `.4`, `.5`, `.500000001` and `.6` s, and `+01:00` spellings
- `an_input_operand_against_now_in_when_subject_agrees_with_the_interpreter`
- `a_row_set_and_an_input_guard_in_one_command_agree_with_the_interpreter`: a many-row `when_related:` and a `when:` in one command, 3×3 combinations, plus a null stored row
- `a_now_guard_beside_a_missing_related_row_agrees_with_the_interpreter`
- `a_now_guard_beside_the_held_state_agrees_with_the_interpreter`
- `a_clock_with_no_instant_agrees_with_the_interpreter`: undecidable on both sides
- `every_guard_of_one_decision_reads_the_clock_once`: kernel only, a clock that moves 10 s per read

Suite: `cargo test -p spindle-kernel`: `test result: ok. 264 passed; 0 failed; 4 ignored`, exit 0.

Attacked and could not break: clock precision (`mint.rs:392`, nanoseconds as ESS's `DecisionInstant`); every guard site passes the run's clock reading (`.at(self.now())`); invariants (`execute.rs:62`, `typed.rs:188`) and set-effect filters (`execute/set.rs:130`) get none, where ESS refuses `now` (`CurrentTimeSite::None`); guards without `now` change by one clock read per run only; the conformance target uses `SystemClock`; the unit's own tests assert literal outcomes and compare with the interpreter.

```findings
[]
```

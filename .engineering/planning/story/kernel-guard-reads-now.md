---
format: aep.planning-md/3
id: story:kernel-guard-reads-now
kind: story
status: draft
title: A command guard ordering against now is decided on the kernel's clock
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/facts.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/guard_reads_now.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/main.rs
revision: 5
---
# A command guard ordering against `now` is decided on the kernel's clock

## Outcome

A command guard that orders a timestamp against `now` (`due_at < now`, `due_at < now - 3600s`) is
decided as ESS 0.90.0's interpreter decides it. The kernel reads its clock once per decision, the
same instant the row-set selector already reads, and hands it to ESS's evaluator in every guard.

The interpreter at the pinned rev (9a2468e, `ess-conformance` `src/interpret/`) reads the decision's
one instant in these places:

- `when:` over the input: `caller::Facts::new(facts, caller, &spec.input, context.now)` in
  `execute.rs` at line 1179.
- `when_subject:`, and `when_related:` over one related row: `Held::selects` (`execute.rs` lines
  1269 and 1292) builds `caller::Facts::new(.., now)` in `execute/subject.rs` at line 124.
- A row-set `when_related:` selection: `execute/row_set.rs` at line 199.

An entity invariant is not read with a clock. The interpreter's `at_rest` (`execute.rs` line 3226)
evaluates it over `history::Facts`, whose `now` is the trait's default `None` (`ess-primitives`
`src/facts.rs` line 1209). ESS validation also refuses `now` in an invariant, a view filter's
pre-`ess/30` form, a selection or a set-effect filter (`ess-domain` `src/expression.rs`,
`CurrentTimeSite::None` at line 545 and the refusal at line 2760). `spindle-spec` loads through
`ess-domain`, so no such model reaches admission, and this story adds no admission refusal.

Today `impl FactSource for Facts` (`crates/spindle-kernel/src/facts.rs`, line 268) supplies no
`now`. A `when:`, `when_subject:` or single-row `when_related:` guard ordering against `now`
therefore evaluates to Unknown at run time. The row-set selector (`Selector`, `now` at line 263)
already supplies the instant, read once per run into `Run::instant`
(`crates/spindle-kernel/src/execute.rs`, line 405) at line 708.

## Acceptance

The fixture is a neutral one in the spindle-kernel test binary, with a fixed clock on both sides. Its
commands are refused `overdue` when a timestamp is before `now`.

- `a_when_guard_ordering_against_now_is_decided_on_the_clock`: `when: input.due_at < now`. An input
  before the clock's instant answers `overdue`, and one after it answers the accepting outcome.
- `a_when_subject_guard_ordering_against_now_is_decided_on_the_clock`:
  `when_subject: due_at < now` over the addressed row's stored `due_at`, the same two sides.
- `a_when_related_guard_ordering_against_now_is_decided_on_the_clock`: `when_related:` over one
  related row whose `due_at < now`, the same two sides.
- `a_guard_ordering_against_now_minus_a_duration_is_decided_on_the_clock`:
  `when: input.due_at < now - 3600s`. An input 3601 s before the instant answers `overdue`, and one
  3599 s before it answers the accepting outcome.
- `now_guards_agree_with_the_interpreter`: for each of the four commands, three steps with the input
  or stored timestamp before, at and after the bound. Each runs on a fresh store at one fixed
  instant, given to the kernel's clock and to the interpreter as its `decision_time`
  (`execute_at`). Outcomes, errors and rows equal the interpreter's. At the bound, `<` does not
  hold, so the step answers the accepting outcome on both sides.

## Scope

- `crates/spindle-kernel/src/facts.rs`: cited. `impl FactSource for Facts` (line 268), which gains
  `now`.
- `crates/spindle-kernel/src/execute.rs`: cited. `Run::instant` (line 405), the selector's read of
  it (line 708), and the guard sites that build `Facts`: `Run::guard` (line 640),
  `Run::subject_facts` (line 1025, read at line 566) and `Run::related_facts` (line 809, read at
  line 613).
- `crates/spindle-kernel/tests/spindle-kernel/guard_reads_now.rs`: inferred, new, with its fixture.
- `crates/spindle-kernel/tests/spindle-kernel/main.rs`: inferred. The `mod guard_reads_now;` line.

## Order

- `crates/spindle-kernel/src/execute.rs` is shared with `story:kernel-set-outcomes`,
  `story:kernel-secondary-effects`, `story:state-change-conditions`,
  `story:response-field-value-source`, `story:kernel-creation-takes-unknown-row`,
  `story:kernel-row-set-commit-conflict` and `story:error-payload-generated-fields`.
- `crates/spindle-kernel/tests/spindle-kernel/main.rs` is shared with
  `story:kernel-creation-takes-unknown-row`, which adds a `mod` line too.

## Origin

Pre-existing. It was found while fixing the same gap for view filters in
`story:view-field-literal-filters` (wave 14).

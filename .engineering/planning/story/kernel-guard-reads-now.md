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
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/guard_reads_now.rs
revision: 2
---
# A command guard ordering against `now` is decided on the kernel's clock

## Outcome

A guard, selection or invariant that orders a timestamp against `now` (`due_at < now`,
`now - 3600s`) is decided as ESS's interpreter decides it: the kernel reads its clock once per
decision and hands it to ESS's evaluator, or, where ESS's interpreter does not read a clock for that
construct, the kernel refuses it by name at admission. Today `impl FactSource for Facts`
(`crates/spindle-kernel/src/facts.rs`) supplies no `now`, so such a `when:` guard evaluates to
Unknown at run time; the row-set selector (`Selector`) already supplies it.

## Acceptance

A neutral fixture whose command is refused `overdue` when an input timestamp is before `now`, in
the spindle-kernel test binary, with a fixed clock on both sides:

- `a_guard_ordering_against_now_is_decided_on_the_clock`: before and after the instant, the
  outcome differs as ESS's interpreter's does.
- `now_guards_agree_with_the_interpreter`: outcomes, errors and rows over the same steps equal the
  interpreter's.

## Scope

- `crates/spindle-kernel/src/facts.rs` — cited: `impl FactSource for Facts`.
- `crates/spindle-kernel/src/execute.rs` — inferred: the decision's instant.
- `crates/spindle-kernel/tests/spindle-kernel/` — inferred: a new module and fixture.

## Origin

Pre-existing; found while fixing the same gap for view filters in
`story:view-field-literal-filters` (wave 14).

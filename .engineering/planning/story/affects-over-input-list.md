---
format: aep.planning-md/3
id: story:affects-over-input-list
kind: story
status: draft
title: 'An affects: entry over an input list runs once per element'
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/outcome_scoped_refusal.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
revision: 2
---
# An `affects:` entry over an input list runs once per element

## Outcome

The kernel runs an outcome whose `affects:` entry walks an input list
(`each: {in: input.<list>, as: <name>}`): for each element, in list order, the entry creates or
updates one row of its entity, with values read from the element through its binder, as ESS
0.99.2's interpreter does ("one record per input-list element"). `admit`, `run` on stdin and the
HTTP ingress answer such a command instead of refusing it. Today the kernel refuses the outcome
before it runs: `crates/spindle-kernel/src/supported.rs:629` answers 501 "unsupported: an
`affects:` entry over an input list (`each:`)".

## Acceptance

- `an_affects_entry_over_an_input_list_creates_one_row_per_element`: a command whose input list
  holds three elements creates three rows of the affected entity, each with the values of its
  element; the kernel answers as ESS's interpreter for the same steps.
- `an_affects_entry_over_an_empty_input_list_creates_no_row`: the subject's effect still applies.
- `an_affects_entry_over_an_input_list_updates_the_rows_it_names`: elements naming rows already
  held update them.
- `admit_lists_no_refusal_for_an_affects_entry_over_an_input_list`: `admit` on a specification with
  such an outcome lists it as served.
- `run_answers_an_affects_entry_over_an_input_list_on_stdin_and_http`: one stdin line and one HTTP
  request each create the rows and answer the outcome.
- The case in `crates/spindle-cli/tests/spindle-cli/outcome_scoped_refusal.rs:522`, which pins the
  refusal, changes to the served answer.

## Consumer gap

Recorded 2026-11-27. A consumer's specification records one row per element of an input list
when a component version is released (`affects:` with `each: {in: input.contracts, as: c}`).
Spindle 0.22.0 with ESS 0.99.2 refuses it at admission with the 501 above.

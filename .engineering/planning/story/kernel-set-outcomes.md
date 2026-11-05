---
format: aep.planning-md/3
id: story:kernel-set-outcomes
kind: story
status: active
title: Set outcomes
relations:
- serves: vision:spindle
- decomposes: epic:downstream-kernel-constructs
scope:
- confidence: inferred
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/store.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: inferred
  path: crates/spindle-kernel/src/values.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/set_outcomes.rs
revision: 8
transitions:
- {from: "draft", to: "proposed", at: "2026-11-05T12:00:30Z", actor: "human:timo", revision: 7}
- {from: "proposed", to: "active", at: "2026-11-05T12:00:30Z", actor: "human:timo", revision: 8}
---
# Set outcomes

## Outcome

The kernel runs an outcome that declares `instances:`: it selects every row of the named entity
whose filter holds, read from the store as it was before the outcome, and moves, updates or
removes each one in ascending order of identity. A row resting outside a move's `from` states is
skipped, and zero rows selected is a successful outcome. `{changed_count: …}` reads how many rows
changed, and an event declared once per changed row is published once for each, carrying that
row's fields as they were before the outcome (`changed_field`). Today
`crates/spindle-kernel/src/supported.rs` (`shape`) refuses every such outcome as "a set outcome
(`instances:`)".

Requested by a downstream specification probe (2026-11-05, 44 commands): 6 commands are refused
first on this construct and a seventh declares it behind a row-set guard. Reproduced with a
neutral fixture.

ESS semantics: ESS 0.86.0's interpreter, `crates/verify/ess-conformance/src/interpret/execute/set_effects.rs`
(the changed rows and their count) and `execute/row_set.rs` (selection from the pre-outcome store).

## Acceptance

Fixture: a library whose `MarkOverdue` command moves every loan with `due <= input.as_of` from
`Out` to `Overdue`, publishes `LoansMarked {count: {changed_count}}` and one `LoanMarked` per
changed loan.

- `a_set_outcome_moves_every_selected_row` — three loans, two due: both move, the third stays.
- `rows_outside_the_move_s_from_states_are_skipped` — a loan already `Returned` and due is not
  moved and not counted.
- `zero_selected_rows_is_a_successful_outcome` — the outcome is taken with a count of 0 and no
  per-row event.
- `one_event_per_changed_row_in_identity_order` — the per-row events carry each row's fields as
  they were before, in ascending identity order.
- `set_outcomes_agree_with_the_interpreter` — for the same steps, the kernel's outcome, published
  events and resulting rows equal what ESS 0.86.0's interpreter produces.
- `a_set_outcome_survives_a_restart_on_sqlite` — the selection reads the persisted rows.

## Scope

- `crates/spindle-kernel/src/supported.rs` — cited: the `instances` refusal in `shape`.
- `crates/spindle-kernel/src/execute.rs` — inferred: taking the outcome over several rows.
- `crates/spindle-kernel/src/store.rs` — cited: `Store::scan`, the candidate rows.
- `crates/spindle-kernel/src/values.rs` — inferred: `changed_count` and `changed_field` sources.
- `crates/spindle-kernel/tests/set_outcomes.rs` — inferred, new.

## Depends on

Nothing. Shares row selection over `Store::scan` with `story:kernel-secondary-effects` and
`story:kernel-row-set-guards`; whichever lands first owns the selector evaluation.

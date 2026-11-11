---
format: aep.planning-md/3
id: story:kernel-secondary-effects
kind: story
status: implemented
title: Secondary effects
relations:
- serves: vision:spindle
- decomposes: epic:downstream-kernel-constructs
- depends_on: story:kernel-set-outcomes
scope:
- confidence: inferred
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/store.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/secondary_effects.rs
revision: 8
transitions:
- {from: "draft", to: "proposed", at: "2026-11-05T12:00:33Z", actor: "human:timo", revision: 6}
- {from: "proposed", to: "active", at: "2026-11-05T12:00:33Z", actor: "human:timo", revision: 7}
- {from: "active", to: "implemented", at: "2026-11-11T20:31:04Z", actor: "human:timo", revision: 8, decided_on: {"recorded":{"test_result":1}}}
---
# Secondary effects

## Outcome

The kernel runs an outcome that declares `affects:`: after the subject's own effect, each entry
selects the rows of its entity whose `filter:` holds, read from the store as it was before the
outcome, the subject itself excepted, and moves them (a row outside the move's `from` states is
skipped), removes them (`deletes: true`) or updates them with the entry's `sets:`, entries applied
in declaration order and every row left in place checked at rest. Today
`crates/spindle-kernel/src/supported.rs` (`shape`) refuses it as "secondary effects
(`affects:`)".

Requested by a downstream specification probe (2026-11-05, 44 commands): 2 commands are refused
first on this construct, one moving the selected rows with `sets:`, one removing them.
Reproduced with a neutral fixture.

ESS semantics: ESS 0.86.0's interpreter, `crates/verify/ess-conformance/src/interpret/execute/set_effects.rs`
(the plans applied in declaration order).

## Acceptance

Fixture: a notebook whose `ArchiveNotebook` command moves the notebook to `Archived`, moves every
page with `notebook_id == input.notebook_id` from `Open` to `Archived` setting
`archived_note: "notebook archived"`, and removes every draft with the same `notebook_id`.

- `an_affects_entry_moves_the_selected_rows` — the notebook's open pages move and carry the note;
  another notebook's pages do not.
- `a_row_outside_from_is_skipped` — an already archived page is left as it was.
- `a_deleting_entry_removes_the_selected_rows` — the drafts are gone; a view lists none.
- `entries_apply_in_declaration_order_over_the_rows_before` — a row an earlier entry changed is
  selected by a later entry as it was before the outcome.
- `secondary_effects_agree_with_the_interpreter` — for the same steps, outcomes, events and rows
  equal what ESS 0.86.0's interpreter produces.

## Scope

- `crates/spindle-kernel/src/supported.rs` — cited: the `affects` refusal in `shape`.
- `crates/spindle-kernel/src/execute.rs` — inferred: `take` and `write`, several rows per change.
- `crates/spindle-kernel/src/store.rs` — cited: `Change`, now carrying several rows and removals.
- `crates/spindle-kernel/tests/secondary_effects.rs` — inferred, new.

## Depends on

- `story:kernel-set-outcomes` — the same filtered selection and multi-row write.

---
format: aep.planning-md/3
id: story:related-selector-rows-quantifier
kind: story
status: active
title: A when_related row-set selector that walks an entity's rows is decided
relations:
- serves: vision:spindle
revision: 8
transitions:
- {from: "draft", to: "proposed", at: "2026-11-26T18:58:23Z", actor: "human:operator", revision: 7, decided_on: {"recorded":{"review_outcome":14}}}
- {from: "proposed", to: "active", at: "2026-11-26T18:58:23Z", actor: "human:operator", revision: 8, decided_on: {"recorded":{"review_outcome":14}}}
---
# A when_related row-set selector that walks an entity's rows is decided

## Outcome

A `when_related:` row-set condition whose selector or test holds a rows quantifier
(`exists: {in: {entity: …}, …}` or `forall:` over the rows of an entity, ess/27) is decided as
ESS's interpreter decides it, and the command is served instead of refused at start. Today
`crates/spindle-kernel/src/supported.rs:389` refuses it as "a rows quantifier in a `when_related:`
row-set selector", because the kernel reads the row set from the store before selection and does
not walk a second entity's rows inside the selector.

Reported 2026-11-26 by a consumer: a usage-charging command guarded against rows of another
organization was refused by Spindle 0.21.0, and the consumer rewrote its guard to get served. The
same refusal stands for an `affects:` filter (`supported.rs:464`) and an `instances:` filter
(`supported.rs:486`); this story covers the `when_related:` selector, and names whether the other two
share its fix.

## Acceptance

- `a_rows_quantifier_in_a_related_selector_decides_as_the_interpreter`: a command whose
  `when_related:` row-set selector holds `exists: {in: {entity: …}, …}` over a second entity selects
  the outcome ESS 0.99.1's interpreter selects, for a request where a matching row exists and one
  where none does.
- The same with the rows `exists` inside the row-set test's `forall:` (ESS admits `exists` over
  rows only).
- `a_write_to_the_walked_entity_conditions_the_commit`: a row of the walked entity written between
  the read and the commit makes the commit retry or refuse as a write to the selected entity does.
- `admit` and `run` serve the command; the route answers the interpreter's status, not 501.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel`, the row-set guard path — cited
- **Files:** `crates/spindle-kernel/src/supported.rs:380-391` (the `RelatedSet` refusal arm in `condition()`, removed) — cited
- **Files:** `crates/spindle-kernel/src/execute.rs:905-982` (`Run::row_set` scans only `selection.entity`; it also reads every entity `related_rows::walked` names in the selector and the test, and records each in `self.scanned`, `:563-566`, so the commit is conditioned on that entity's version) — cited
- **Files:** `crates/spindle-kernel/src/facts.rs:211-288` (`Selector` gains the walked rows and `fn rows(&self, entity)`, as ESS's `set_effects::Facts::rows`) — cited
- **Files:** `crates/spindle-kernel/tests/spindle-kernel/row_set_guards.rs:378-419` (`a_rows_quantifier_in_a_selector_is_refused` pins today's refusal and is replaced) — cited
- **Symbols:** `supported::condition`, `ResolvedCondition::RelatedSet`, `Run::row_set`, `facts::Selector`, `Scanned`, `ess_domain::command::related_rows::walked`, `FactSource::rows` (ess-primitives 0.99.1 `src/facts.rs:1659-1668`) — cited
- **Also likely:** `execute.rs:453-470` (`Open`/`Unknowns`); the other `Selector { … }` literals at `execute.rs:1036`, `execute/affects.rs:214`, `execute/set.rs:149`; a fixture with a second entity — inferred
- **Documents:** `website/docs/concepts/specification.md:65-68`, `CHANGELOG.md` — inferred
- **Not touched:** `supported.rs:438-490` (`affects:` and `instances:` refusals, see below); `supported.rs:753-758` (related-read selector, a separate gap) — inferred
- **Confidence:** high — ESS's interpreter shows the shape: it collects walked entities from the filter and the test (ess-conformance `interpret/execute/row_set.rs:156-178`) and lends their rows from one snapshot (`:191-199`)
- **Would collide with:** any unit touching `spindle-kernel` `execute.rs` row-set selection, `facts.rs` `Selector`, `supported.rs` `condition()`
- **Shares the fix:** no. ESS 0.99.1 refuses a rows quantifier in any set-effect filter at validation (ess-domain `command/set_effects.rs:1970`, `:2111-2112`), for `instances:` and `affects:` alike, so those two refusals guard a construct ESS does not accept and stay — cited
- **Note:** ESS admits `exists` over rows only; `forall` over rows is refused with its `not: {exists: …}` rewrite (ess-domain `command/related_rows.rs:11`), so the acceptance's `forall:` case is an `exists` over rows inside the guard's `forall:` test — cited

## Decided when scoped

- Only `when_related:` row-set selectors change. ESS 0.99.1 refuses a rows quantifier in any
  set-effect filter (ess-domain `set_effects.rs:1970`), so the `affects:` and `instances:` refusals
  stay.
- ESS admits `exists` over rows only; the acceptance's `forall:` case is an `exists` over rows inside
  the guard's `forall:` test.

## Shared files

- `crates/spindle-kernel/src/execute.rs`, `facts.rs`, `supported.rs` and the kernel test `main.rs`:
  shared with `story:guard-over-absent-row-field` in the same unit (U1), built after this story on
  one branch. No other wave-26 unit edits `spindle-kernel`.
- `website/docs/concepts/specification.md:65-68`: shared with U2, which extends the view-filter
  leaf list at `:139-155`. Separate hunks, merged by the coordinator.
- `crates/spindle-docs/src/status.rs` and its generated pages, if a row changes: shared with U2, U3
  and U4 on separate rows; the coordinator regenerates the pages after the merges.
- `CHANGELOG.md` `[Unreleased]`: one bullet per story, merged by the coordinator.

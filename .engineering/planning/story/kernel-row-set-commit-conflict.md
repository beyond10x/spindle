---
format: aep.planning-md/3
id: story:kernel-row-set-commit-conflict
kind: story
status: draft
title: A row-set guard holds against a second writer
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/sqlite.rs
- confidence: cited
  path: crates/spindle-kernel/src/store.rs
- confidence: cited
  path: crates/spindle-store-browser
revision: 4
---
# A row-set guard holds against a second writer

## Outcome

A command whose outcome was selected by a `when_related:` guard over a row set does not commit when
another writer committed a row of that set's entity between the kernel's `Store::scan` and the
command's commit: the commit is refused as a conflict and the command is decided again on the new
rows, so the guard's promise (for example, at most one active reservation per reader and book)
holds with two writers on one store. Today the commit's `Change` (`crates/spindle-kernel/src/store.rs`)
checks only the instance it writes, so both writes land.

The store contract gains a version per entity that `scan` returns and a commit can be conditioned
on; the memory, SQLite and IndexedDB stores implement it and the shared store contract tests cover
it. The single-row `when_related:` read is held to the same rule.

## Acceptance

- `a_row_committed_between_the_scan_and_the_commit_does_not_slip_past_the_guard` (spindle-kernel,
  `adversary_row_set`) runs, not ignored, and passes: only the reservation committed first is
  active.
- The store contract suite has `a_commit_conditioned_on_a_scanned_entity_is_refused_after_another_write`,
  run against every store.

## Scope

- `crates/spindle-kernel/src/store.rs` — cited: `Store`, `Change`.
- `crates/spindle-kernel/src/execute.rs` — cited: the commit after selection.
- `crates/spindle-kernel/src/sqlite.rs`, `crates/spindle-store-browser` — inferred: the version per entity.

## Origin

Adversary finding on `story:kernel-row-set-guards` (wave 14): a store wrapper committing a matching
row right after the scan let a second active reservation land. Related:
`story:browser-store-stale-write-conflict`.

---
format: aep.planning-md/3
id: story:kernel-row-set-commit-conflict
kind: story
status: implemented
title: A row-set guard holds against a second writer
relations:
- serves: vision:spindle
- depends_on: story:kernel-set-outcomes
- informed_by: story:browser-store-stale-write-conflict
scope:
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-host/tests/spindle-host/supplied.rs
- confidence: cited
  path: crates/spindle-kernel/src/contract.rs
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/sqlite.rs
- confidence: cited
  path: crates/spindle-kernel/src/store.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/adversary.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/adversary_append.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/adversary_deletes.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/adversary_row_set.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/adversary_store_contract.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/rally_cases/mod.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/related_guards.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/scan.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/store_contract.rs
- confidence: inferred
  path: crates/spindle-store-browser/src/indexeddb.rs
- confidence: inferred
  path: crates/spindle-store-browser/src/lib.rs
- confidence: cited
  path: crates/spindle-store-browser/tests/spindle-store-browser/adversary.rs
- confidence: cited
  path: crates/spindle-store-browser/tests/spindle-store-browser/contract.rs
revision: 24
transitions:
- {from: "draft", to: "proposed", at: "2026-11-07T18:59:08Z", actor: "human:timo", revision: 22}
- {from: "proposed", to: "active", at: "2026-11-07T18:59:08Z", actor: "human:timo", revision: 23}
- {from: "active", to: "implemented", at: "2026-11-08T14:30:57Z", actor: "human:timo", revision: 24, decided_on: {"recorded":{"test_result":1,"verification":1}}}
---
# A row-set guard holds against a second writer

## Outcome

A command whose outcome was selected by a `when_related:` guard over a row set does not commit when
another writer committed a row of that set's entity between the kernel's `Store::scan` and the
command's commit: the commit is refused as a conflict, as a stale single-instance commit already is
(`StoreError::Conflict`, `crates/spindle-kernel/src/store.rs`). The command is not decided again;
the caller sees the refusal. So the guard's promise (for example, at most one active reservation
per reader and book) holds with two writers on one store. Today the commit's `Change`
(`crates/spindle-kernel/src/store.rs`) checks only the instance it writes, so both writes land.

The store contract gains a version per entity that `Store::scan` returns and that a `Change` can be
conditioned on. The memory and SQLite stores implement it, and the shared store contract tests
cover it. The IndexedDB store implements it within one handle, through the per-handle conflict it
already reports on a stale commit (`crates/spindle-store-browser/src/indexeddb.rs`, `commit` at
line 405, `remove` at line 438). Across tabs the refusal is
`story:browser-store-stale-write-conflict`'s.

Selections of a set outcome and `affects:` selections are not conditioned by this story. No story
covers them yet: neither `story:kernel-set-outcomes` nor `story:kernel-secondary-effects` conditions
a multi-row commit on a scanned version. That is an open gap, to be filed as its own story when
either of those is implemented.

## Acceptance

- `a_row_committed_between_the_scan_and_the_commit_does_not_slip_past_the_guard`
  (`crates/spindle-kernel/tests/spindle-kernel/adversary_row_set.rs`, the `#[ignore]` at line 241
  removed) runs and passes. It asserts that exactly one reservation is active, the one committed
  first, and that the losing `Reserve` answers `Err(ExecuteError::Store(StoreError::Conflict { .. }))`.
  Not `Ok`, and not `StoreError::Backend`. Today the test only prints that answer in its assertion
  message (lines 299 to 304) and does not assert it.
- The memory store: a case in `CASES` (`crates/spindle-kernel/src/contract.rs`),
  `a_commit_conditioned_on_a_scanned_entity_is_refused_after_another_write`, passes in
  `the_memory_store_passes_the_store_contract`
  (`crates/spindle-kernel/tests/spindle-kernel/store_contract.rs`). The case scans an entity,
  commits a row of it as a second writer, and then commits a change conditioned on the scanned
  version. That commit answers `StoreError::Conflict` and leaves the entity's rows as the second
  writer left them.
- The SQLite store: the same case passes in `the_sqlite_store_passes_the_store_contract`
  (`crates/spindle-kernel/tests/spindle-kernel/store_contract.rs`).
- The IndexedDB store: the same case passes in the browser store's contract run
  (`crates/spindle-store-browser/tests/spindle-store-browser/contract.rs`), with both writers on
  one handle.

## Scope

- `crates/spindle-kernel/src/store.rs`: cited. `Store::scan` (returns the entity version) and
  `Change` (carries the condition).
- `crates/spindle-kernel/src/execute.rs`: cited. The commit after selection
  (`self.store.commit(change)` in `Run::take`, line 1278) and `write` (line 1707).
- `crates/spindle-kernel/src/sqlite.rs`: cited. `commit` (line 193) and `scan` (line 239).
- `crates/spindle-kernel/src/contract.rs`: cited. `CASES` (line 36).
- `crates/spindle-kernel/tests/spindle-kernel/store_contract.rs` and
  `crates/spindle-store-browser/tests/spindle-store-browser/contract.rs`: cited.
- `crates/spindle-kernel/tests/spindle-kernel/adversary_row_set.rs`: cited. The `#[ignore]` at
  line 241, and the `Racing` store at line 199.
- `crates/spindle-store-browser/src/indexeddb.rs` and `crates/spindle-store-browser/src/lib.rs`:
  inferred.
- Every other `impl Store` and `Change` construction follows the changed shape:
  - `crates/spindle-host/src/host.rs`: cited. `impl Store for Supplied` (line 369) and
    `impl Store for Lost` (line 1577), their `scan` forwarders (lines 386, 1590), and `Change`
    built at lines 801 and 919.
  - `crates/spindle-host/tests/spindle-host/supplied.rs`: cited. `impl Store for Shared` (line 53).
  - `crates/spindle-kernel/tests/spindle-kernel/adversary_store_contract.rs`: cited.
    `impl Store for MisnamesTheHeldVersion` (line 213), `Change` at lines 83 and 95.
  - `crates/spindle-kernel/tests/spindle-kernel/`: cited. `Change` literals in `adversary.rs`
    (line 216), `adversary_append.rs` (line 248), `adversary_deletes.rs` (lines 20, 76),
    `rally_cases/mod.rs` (line 82), `related_guards.rs` (line 292) and `scan.rs` (line 17).
  - `crates/spindle-store-browser/tests/spindle-store-browser/adversary.rs`: cited. `Change` at
    lines 53 and 60.

No new test module is added, so no `mod` line changes.

## Relations

- Depends on `story:kernel-set-outcomes`. Both change `Store::scan` in
  `crates/spindle-kernel/src/store.rs` and the write path in `crates/spindle-kernel/src/execute.rs`,
  and this story runs after it.
- Informed by `story:browser-store-stale-write-conflict`, which owns the refusal across tabs.

## Order

- `crates/spindle-kernel/src/store.rs` is shared with `story:kernel-set-outcomes` and
  `story:kernel-secondary-effects`.
- `crates/spindle-kernel/src/execute.rs` is shared with `story:kernel-set-outcomes`,
  `story:kernel-secondary-effects`, `story:state-change-conditions`,
  `story:response-field-value-source`, `story:kernel-creation-takes-unknown-row`,
  `story:kernel-guard-reads-now` and `story:error-payload-generated-fields`.
- `crates/spindle-host/src/host.rs` is shared with `story:ingress-prometheus-metrics` and
  `story:live-topology`.
- This story runs after `story:kernel-set-outcomes`, and is merged after
  `story:ingress-prometheus-metrics` where both are in one wave.
- `story:browser-store-stale-write-conflict` records no scope. Its fix would land in
  `crates/spindle-store-browser/src/indexeddb.rs` too.

## Origin

Adversary finding on `story:kernel-row-set-guards` (wave 14): a store wrapper committed a matching
row right after the scan, and a second active reservation landed.

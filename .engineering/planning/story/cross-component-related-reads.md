---
format: aep.planning-md/3
id: story:cross-component-related-reads
kind: story
status: active
title: A guard or an effect over another component's entity is refused or read from its owner
summary: Guards and affects entries over an entity another component owns read the running component's store; refuse them by name or read the owner's.
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-cli/src/component.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: inferred
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/admit.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-host/tests/fixtures/cross-component-reads
- confidence: cited
  path: crates/spindle-host/tests/spindle-host/cross_component_reads.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: inferred
  path: crates/spindle-kernel/src/execute/affects.rs
- confidence: inferred
  path: crates/spindle-kernel/src/lib.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/fixtures/metering-split
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/related_rows_quantifier.rs
- confidence: cited
  path: docs/design/spindle.md
- confidence: cited
  path: website/docs/concepts/specification.md
revision: 32
transitions:
- {from: "draft", to: "proposed", at: "2026-11-26T21:00:29Z", actor: "human:operator", revision: 31, decided_on: {"recorded":{"review_outcome":20}}}
- {from: "proposed", to: "active", at: "2026-11-26T21:00:29Z", actor: "human:operator", revision: 32, decided_on: {"recorded":{"review_outcome":20}}}
---
# A guard or an effect over another component's entity is refused or read from its owner

## Outcome

A command whose value source (`{related: ...}` in an outcome), guard or row set reads an entity
another component owns is answered from the owner's store when both components run in one host, as
ESS's interpreter answers it. In a split run, where the owner is another process, it is refused by
name before anything is read. An `affects:` entry over another component's entity is refused by name
in every run.

## Why

The kernel reads and writes only the store of the component that runs the command. ESS's
interpreter holds every entity in one store. Values are covered: `spindle_kernel::takes`
(`crates/spindle-kernel/src/supported.rs`) refuses an outcome whose `{related: {entity, where,
field}}` or `{related: {via: input.<field>, field}}` reads an entity the running component does
not own, with an `unsupported: …` reason (`crates/spindle-host/tests/spindle-host/cross_component_reads.rs`).
Three sites with the same limit are not covered, because selection reads them and the
selection-time check (`spindle_kernel::selects`) takes no IR to find an owner:

- a `when_related:` guard over a row the input or a stored field names (`Run::select`,
  `crates/spindle-kernel/src/execute.rs`): the row is loaded from the running component's store,
  so it is missing and the `exists: false` branch answers where the interpreter finds the row;
- a row-set `when_related:` guard (`Run::row_set`): the entity is scanned in the running
  component's store and finds no row;
- an `affects:` entry over an entity another component owns (`execute/affects.rs`): it selects and
  writes rows in the running component's store.

Found by a review of `0.19.0..0.19.1`, beside the value refusal.

## Acceptance

- `a_value_over_another_components_entity_is_read_from_its_owner`: on the same fixture in one
  host, an outcome whose value source is `{related: ...}` over the other component's entity
  answers the value ESS's interpreter answers, read from the owner's store; this is the read a
  consumer was refused at admission.
- `a_guard_over_another_components_row_is_read_from_its_owner`: on a two-component fixture served in
  one host, a command with a `when_related:` guard over the other component's entity selects the
  outcome ESS's interpreter selects, the row read from the owner's store; `admit` without
  `--component` lists it served.
- `a_row_set_guard_over_another_components_entity_is_read_from_its_owner`: the same for a row-set
  guard with a rows quantifier.
- `a_split_run_refuses_a_read_of_another_components_entity_by_name`: `admit --component <runner>`
  and a `play --component` run name the command not served, naming the entity, its owner and the
  running component.
- `an_affects_entry_over_another_components_entity_is_refused_by_name`: refused by name in a one-host
  run and in a split run, writing no row (D2 A; today nothing refuses it).
- `a_guard_over_the_running_components_entity_is_served`: a guard over the running component's own
  entity answers as before.

- `a_stdin_run_reads_another_components_entity`: the value-read and guard commands above, sent as
  stdin lines on the same fixture, answer what ESS's interpreter answers and are not refused at
  start.
- `a_served_run_answers_a_cross_component_guard`: the fixture with one component `reached_by:
  network`, served by `run` on loopback; an HTTP request for the guarded command answers the
  interpreter's outcome with its status, not 501, and `GET /api/served` lists the route served.

## Not covered

- Reading another component's store when the owner runs in another process (a split run): kept
  refused by name.
- Writing another component's entity (`affects:`): refused by name in every run.

## Joined 2026-11-26

A consumer, 2026-11-26, on Spindle 0.21.0, split one domain into its own component and was refused
at admission: "unsupported: the value source `<entity>.<field>` … owned by component `<a>`: the
kernel reads only the store of component `<b>`, which runs the command". Every component of that
run is in one process (stdin or one served host), so the owner's store is open beside the running
component's. The wanted answer is the first branch of this story's Outcome: read the value, the
guard's row and the row set from the owner's store when both components run in one host, and keep
the refusal only for a split run where the owner is another process.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel/src/supported.rs` (refusal of foreign reads) and
  `crates/spindle-host/src/host.rs` (the store the kernel is handed) — cited
- **Files:**
  - `crates/spindle-kernel/src/supported.rs`: `foreign` :238, `foreign_read` :190, `foreign_guard`
    :270, `takes` :73, `check` :43, `affects` :481. `foreign` decides "foreign" from IR ownership
    alone, so a one-host run is refused today — cited
  - `crates/spindle-kernel/src/execute.rs`: `execute_refusing` :226, `outcome_runs` :311,
    `none_runs` :327, `Run.scanned`. Every read goes through the one `&mut dyn Store`: `load`
    :292/:929/:936/:1268/:2034, `scan` :1081/:1123, `counts` :2049 — cited
  - `crates/spindle-host/src/host.rs`: `Host::execute` :1763 wraps one component's store in
    `Tallied` :391. `loading_from` :2280 and `store_of` :2537 already read the owner's store for
    views, which is the precedent to copy — cited
  - `crates/spindle-cli/src/main.rs`: `unsupported_outcomes` :906, called by `admit` :178 (with
    an optional component) and by the stdin run :722 (`None`). These produce the line the consumer
    hit — cited
  - `crates/spindle-ingress/src/surface.rs`: `refuse` :1677 calls `spindle_kernel::takes`. The
    surface knows only its served components, not which components share the host — cited
- **Symbols:** `spindle_kernel::takes`, `spindle_kernel::runs` (re-export at `lib.rs:95`),
  `supported::foreign`, `foreign_guard`, `foreign_read`, `Host::execute`, `Tallied`, `store_of`,
  `unsupported_outcomes`, `surface::refuse`, `Scanned` — cited
- **Also likely:**
  - `crates/spindle-host/src/host.rs`: a new `Store` wrapper that routes `load`/`scan`/`counts`
    by entity to the owner's component store and sends writes to the running component. It
    replaces `Tallied` in `Host::execute` — inferred
  - `crates/spindle-kernel/src/lib.rs`: re-export of a widened `takes`/`runs` — inferred
  - `crates/spindle-cli/src/serve.rs` (:324, :334, `Surface::*_callers`) and
    `crates/spindle-cli/src/component.rs` (:83 `Host::component`, :259 `Surface::new`). These
    tell the surface which components share the host: all of them in `serve`, one in `component`
    — inferred
  - `crates/spindle-kernel/src/execute/affects.rs`: untouched under D2 A, because the refusal
    lives in `supported::affects`. Under D2 B it changes — inferred
- **Tests:**
  - `crates/spindle-host/tests/spindle-host/cross_component_reads.rs`: the two
    `…_is_refused_by_name` tests flip to `…_is_read_from_its_owner` (one host, `Host::new`; the
    file's `interpreter_issues` already compares against ESS) — cited. New in the same file:
    - `a_guard_over_another_components_row_is_read_from_its_owner`
    - `a_row_set_guard_over_another_components_entity_is_read_from_its_owner` (with a rows
      quantifier, and the decided row committed)
    - `an_affects_entry_over_another_components_entity_is_refused_by_name`
    - `a_split_run_refuses_a_read_of_another_components_entity_by_name`
    - `a_guard_over_the_running_components_entity_is_served`
    — inferred names
  - `crates/spindle-kernel/tests/spindle-kernel/related_rows_quantifier.rs`:
    `a_guard_over_another_components_rows_is_refused_by_name` :174 becomes the split-run case of
    the kernel check — cited
  - `crates/spindle-kernel/tests/fixtures/metering-split` (ChargeHeld = row, ChargeUsage /
    ChargeDirect = row set and quantifier) is reused by host tests. `cross-component-reads` gains
    one command with a foreign `affects:` entry; neither fixture has one today — inferred
  - `crates/spindle-cli/tests/spindle-cli/admit.rs`: `admit` without `--component` lists no
    "not served" for the cross-component command; with it, it does — inferred
  - All are in-memory, one host each, expected well under 60 s each. The test step stays within
    120 s; not timed — inferred
- **Documents:**
  - `website/docs/concepts/specification.md` :93 ("The kernel reads only the store of …") is
    rewritten for one host vs. split — cited
  - `docs/design/spindle.md` § 11 gets a decision row, as the story's "Not covered" asks — cited
  - `CHANGELOG.md` — inferred
- **Confidence:** medium. The read sites and refusal sites are cited. How the "which components
  share this host" fact reaches `takes` and the surface is a design choice (D1), and it moves
  which files change.
- **Would collide with:**
  - `spindle-ingress/src/surface.rs` constructors and `refuse`, which units touching the
    surface's routes also touch;
  - `spindle-cli/src/main.rs` stdin-run and `admit` paths;
  - `spindle-cli/src/serve.rs` surface construction;
  - `spindle-host/src/host.rs` `Host::execute`;
  - `spindle-kernel/src/supported.rs`, which any unit changing what the kernel refuses also
    touches.
- **Safety fact:** in one host, every kernel read of another component's rows goes through
  `Store::load`/`scan`/`counts` on the store `Host::execute` hands it (`execute.rs` sites above,
  `values.rs:99`, `:112`, `affects.rs:63`, `set.rs:132`). So a routing store answers them all
  without a kernel read path changing. Level 2, unproven.
- **Safety fact (to prove):** a write decided on a foreign scan carries that entity in
  `Write::scanned`, and the runner's store checks it against its own version (`sqlite.rs:502`
  `unmoved`, :562, :704). Passed through unchanged, it fails as "moved" or checks the wrong store.
  The router must take foreign `Scanned` out and check them against the owner. That is safe only
  while no other writer touches the owner's store between the scan and the commit; `Host` is
  `&mut self`, but another process on the same durable store is not ruled out. Level 2, unproven.

## Decided when scoped

Taken 2026-11-26 on the scoper's recommended option, each reversible, unless noted.

- D2 A: an `affects:` entry over another component's entity is refused by name in
  every run (today nothing refuses it).
- D1 A, answered by the operator through the host operator, 2026-11-26 about 21:15 CET: the
  kernel's admission checks (`takes`, `check`, `execute_refusing`) take the set of components the
  store reaches, and a routing store in `Host::execute` reads each entity from its owner's store;
  `admit` passes all components, or the one `--component`; `component` passes one. A row in
  `docs/design/spindle.md` § 11 records it.
- The one-host branch of the Outcome is the one delivered; a split run keeps the refusal by name.
- The widening is additive: `takes`, `check`, `execute_refusing`, `execute_as` and the
  `Surface` constructors keep their signatures and treat only the running component as reached;
  new functions beside them (`takes_reaching`, `execute_reaching`, a `Surface` builder taking
  the reached set) carry the set. Their other callers (`spindle-control`, `spindle-embed`, the
  kernel and CLI test files, `main.rs:882`) stay unchanged.
- `crates/spindle-kernel/src/lib.rs`: the new re-exports sit beside `runs` (`:95`); U5 adds its
  export beside `admits` (`:104`); U5 merges first.
- A stdin run passes every component as reached: its one store holds every component's rows
  (`main.rs:722`, `unsupported_outcomes` with `None`).

---
format: aep.planning-md/3
id: story:affects-over-input-list
kind: story
status: active
title: 'An affects: entry over an input list runs once per element'
relations:
- serves: vision:spindle
- depends_on: story:decimal-identity-any-spelling
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/affects-each-network
- confidence: cited
  path: crates/spindle-cli/tests/fixtures/outcome-scoped-refusal-effects/domains/lot.yaml
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/outcome-scoped-refusal-effects/system.yaml
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/outcome_scoped_refusal.rs
- confidence: inferred
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/execute/affects.rs
- confidence: inferred
  path: crates/spindle-kernel/src/facts.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/fixtures/affects-each
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/affects_each.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/main.rs
- confidence: cited
  path: website/docs/concepts/specification.md
revision: 18
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T10:34:38Z", actor: "human:operator", revision: 17, decided_on: {"recorded":{"review_outcome":9}}}
- {from: "proposed", to: "active", at: "2026-11-27T10:34:38Z", actor: "human:operator", revision: 18, decided_on: {"recorded":{"review_outcome":9}}}
---
# An `affects:` entry over an input list runs once per element

## Outcome

The kernel runs an outcome whose `affects:` entry walks an input list
(`each: {in: input.<list>, as: <name>}`) as ESS 0.99.2's interpreter does
(`ess-conformance` `interpret/execute/set_effects.rs`, `elements`): for each element, in list
order, the entry acts on the row the element names. With `sets:` it updates that row if it is held
and creates it in the entity's `initial` state if not ("one record per input-list element"); a
`moves:` or `deletes:` entry acts only on a held row. An element the entry's `where:` does not
select is skipped, an element naming the subject is skipped, and `repeated: last_wins` keeps the
last element of a repeated row. `admit`, `run` on stdin and the HTTP ingress answer such a command
instead of refusing it. Today the kernel refuses the outcome before it runs:
`crates/spindle-kernel/src/supported.rs:629` answers 501 "unsupported: an `affects:` entry over an
input list (`each:`)", and `execute/affects.rs:189-197` refuses it again at run time.

The other two refusals of `supported::affects` stay: ESS refuses both itself (a rows quantifier in
an `affects:` filter, and `affects:` beside a creating or missing subject).

## Acceptance

- `an_affects_entry_over_an_input_list_creates_one_row_per_element`: a command whose input list
  holds three elements creates three rows of the affected entity, each with the values of its
  element.
- `an_affects_entry_over_an_empty_input_list_creates_no_row`: the subject's effect still applies.
- `an_affects_entry_over_an_input_list_updates_the_rows_it_names`: an element naming a held row
  updates it and creates no second row.
- `an_affects_entry_over_an_input_list_moves_only_held_rows`: a `moves:` entry moves the held rows
  its elements name and creates none for the others.
- `an_affects_entry_over_an_input_list_deletes_only_held_rows`: a `deletes:` entry removes the held
  rows its elements name and leaves the store otherwise unchanged.
- `an_element_its_filter_does_not_select_is_skipped` and `an_element_naming_the_subject_is_skipped`.
- `affects_over_an_input_list_agrees_with_the_interpreter`: for a sequence of commands over these
  cases, including `repeated: last_wins` and an element whose `Decimal` identity is written `1.0`
  for a row held as `1`, every answer, row and event equals ESS's interpreter. The `Decimal` case
  reads the one spelling of `story:decimal-identity-any-spelling`: the coordinator merges
  `wave/30` holding U6 into U11's branch before U11's last run, and U11 reports green only on
  that tree.
- `admit_lists_no_refusal_for_an_affects_entry_over_an_input_list`: `admit` on the
  `outcome-scoped-refusal-effects` fixture lists its `swept` outcome as served.
- `stdin_runs_an_affects_entry_over_an_input_list`: on the `outcome-scoped-refusal-effects`
  fixture, a stdin line that selects `swept` answers `swept`. Following lines send a new command
  `bay.lot.SweepTag` (added to `lot.yaml`, with a `wrong-state` outcome for a tag already in
  `Swept`) for each tag: every held tag the input listed answers `wrong-state`, every other
  tag answers as a `Live` one.
- `http_serves_an_affects_entry_over_an_input_list`: on a new fixture
  `crates/spindle-cli/tests/fixtures/affects-each-network/`, whose component is
  `reached_by: network` and whose outcome declares an event in its own `emits:`, one HTTP
  request that selects that outcome answers it, and
  `GET /api/history?entity=<affected entity>&identity=<id>` for each element answers that event
  for the row the element created or moved (`/api/history` answers a write's events for every
  row of its write group, `Committed::instances`).
- `an_outcome_with_secondary_effects_refuses_only_the_request_that_selects_it`
  (`crates/spindle-cli/tests/spindle-cli/outcome_scoped_refusal.rs:568`) keeps testing an
  outcome-scoped refusal on stdin, on a new outcome of `CloseLot` with `compensates: true`, which
  the kernel still refuses and ESS accepts (as in the fixture
  `adversary-outcome-scoped-refusal-compensates`).
- `website/docs/concepts/specification.md:124-125` no longer names `each:` among the constructs
  Spindle refuses.

## Consumer gap

Recorded 2026-11-27. A consumer's specification records one row per element of an input list
when a component version is released (`affects:` with `each: {in: input.contracts, as: c}`).
Spindle 0.22.0 with ESS 0.99.2 refuses it at admission with the 501 above.

## Scope

Derived 2026-11-27 by `story-scoper` against `plan/wave-30` (cbf7ec1e). Every line is **cited** (read from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel` — cited (`supported.rs:629`; `execute/affects.rs:189-197` returns the same refusal at run time)
- **Files:** `crates/spindle-kernel/src/supported.rs:602-636` (`affects`: drop the `each:` refusal, keep the "beside" and rows-quantifier refusals) — cited
- **Files:** `crates/spindle-kernel/src/execute/affects.rs` (`Run::affect`, `select_affected`: one row per element in list order; update if the row exists, create in `initial` if not, with `Change { expected: None }`; moves and deletes only for existing rows; skip the subject; apply the element filter; write `each.reads`) — cited (`:189` holds the run-time refusal)
- **Files:** `crates/spindle-cli/tests/spindle-cli/outcome_scoped_refusal.rs:519-608` (the `SWEPT` constant at `:524`, the test at `:568`) and `crates/spindle-cli/tests/fixtures/outcome-scoped-refusal-effects/domains/lot.yaml:84-100` — cited
- **Symbols:** `supported::affects`, `Run::affect`, `Run::select_affected`, `ess_compiler::ir::ResolvedEach` (`list`, `binder`, `member`, `reads`, `filter`, `repeated`), `ResolvedElementRead` — cited
- **Parity target:** ESS 0.99.2 `crates/verify/ess-conformance/src/interpret/execute/set_effects.rs` `apply`, `carry_out`, `elements`, `ElementFacts` (unchanged since 0.99.1) — cited
- **Also likely:** `crates/spindle-kernel/src/facts.rs` — inferred: a rebinding fact source for `<binder>.<member>` → `<list>.<index>.<member>`; input lists already bind at `<path>.<index>` (`facts.rs:203-216`). It may instead live in `execute/affects.rs`
- **Also likely:** `crates/spindle-kernel/src/execute.rs` — inferred: doc comment at `:87` at most; the call sites `:1968` and `execute/delete.rs:38` keep their signature
- **Tests (new):** `crates/spindle-kernel/tests/spindle-kernel/affects_each.rs` plus a line in `tests/spindle-kernel/main.rs`, and a new fixture `crates/spindle-kernel/tests/fixtures/affects-each/` (format at least ess/27; ess/35 for `repeated:`) — inferred. Named tests: `an_affects_entry_over_an_input_list_creates_one_row_per_element`, `an_affects_entry_over_an_empty_input_list_creates_no_row`, `an_affects_entry_over_an_input_list_updates_the_rows_it_names`, `an_affects_entry_over_an_input_list_moves_only_held_rows`, `an_affects_entry_over_an_input_list_deletes_only_held_rows`, `an_element_its_filter_does_not_select_is_skipped`, `an_element_naming_the_subject_is_skipped`, `affects_over_an_input_list_agrees_with_the_interpreter` (with `ess_conformance::interpret`, like `secondary_effects.rs:392`). In-process stores, each expected under 1 s, not measured
- **Tests (cli):** `admit_lists_no_refusal_for_an_affects_entry_over_an_input_list`, `stdin_runs_an_affects_entry_over_an_input_list` (with the new `bay.lot.SweepTag` in the fixture `lot.yaml` and in `accepts.commands` of its `system.yaml`) and `http_serves_an_affects_entry_over_an_input_list` (on the new fixture `affects-each-network/`), all three in `outcome_scoped_refusal.rs`, so the `spindle-cli` test `main.rs` gains no line — inferred. Each starts the binary, a few seconds each, not measured; well under the 60 s per test and the 120 s step
- **Documents:** `website/docs/concepts/specification.md:124-125` (names `each:` as the refused example), `CHANGELOG.md` `[Unreleased]` — cited
- **Confidence:** high for the kernel surface (the story and the tree name both refusal sites); medium for where the tests and the fixture land
- **Would collide with:** `supported.rs` (wave 29 changes it; wave 30 U3 changes `unfilled_error_field`, a different hunk); `execute/affects.rs` with U5, which adds a field to `ExecuteError::Undeclared` at `:240` that every `Undeclared` constructor in U11's rewritten `Run::affect` sets, so U11 merges after U5; `execute.rs` (U3, U4, U5, U6) at most a doc comment at `:87`, near no other unit's hunk; `website/docs/concepts/specification.md` with U1 (`:168-173`, U11 at `:124-125`); the `spindle-kernel` test `main.rs` module list (U3, U6); `CHANGELOG.md` `[Unreleased]` (every unit). `facts.rs` and the cli refusal fixture are named by no other wave-29 or wave-30 surface
- **Safety fact:** creating a row inside the outcome's write group with `Change { expected: None }` is the existing creation contract of `Store::commit_all`: a row that appeared after the scan refuses the whole group — `store.rs:47`, `:321-328`, `sqlite.rs:608-637`, level 2, unproven
- **Tests (fixture, new):** `crates/spindle-cli/tests/fixtures/affects-each-network/`, a component
  `reached_by: network` with one outcome whose `affects:` walks an input list, for the HTTP case;
  the `outcome-scoped-refusal-effects` fixture is `reached_by: in_process` and `run` serves HTTP
  only for a network component — cited (`system.yaml:23`, `website/docs/reference/cli.md:107`)

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible.

- D1 A: the `swept` outcome of the `outcome-scoped-refusal-effects` fixture becomes the served
  case; the stdin refusal test moves to a new outcome of `CloseLot` with a construct the kernel
  still refuses and ESS accepts.
- D2 A: all of ESS's `each:` on `affects:` is served: create or update, moves and deletes of held
  rows only, the element filter, `repeated: last_wins`, and skipping the subject.
- An element's identity read from inside the list gets the one spelling of
  `story:decimal-identity-any-spelling`; this story depends on it and merges after it.

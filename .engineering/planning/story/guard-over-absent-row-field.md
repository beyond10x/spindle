---
format: aep.planning-md/3
id: story:guard-over-absent-row-field
kind: story
status: draft
title: A guard over an absent optional field of a held row is decided as ESS decides it
relations:
- serves: vision:spindle
revision: 9
---
# A guard over an absent optional field of a held row is decided as ESS decides it

## Outcome

A command guard that compares an `Optional` field of the held instance (or of a related row) which
was never written decides as ESS 0.99.1's interpreter decides it: the comparison is Unknown and no
outcome is selected, so the command is refused as `undecidable` in both. Spindle's refusal then
names the guard and the absent field and suggests `defined(<field>)`, so the specification's author
can write the guard ESS decides. Today the refusal reads "…is undecidable over the held instance and
the input" and names neither.

Reported 2026-11-26 by two consumers on Spindle 0.21.0, at run time: three guards, each comparing an
optional field the held or related row never had (a claim reference, a boolean flag, a reference
to an assigned resource), answered `undecidable` with no hint of the field.

## Acceptance

- `a_guard_over_an_absent_optional_field_decides_as_the_interpreter`: for each of `field == x`,
  `field != x`, `field in [..]`, ordering, `defined(field)` and `not (...)` over an `Optional`
  field of the held instance that was never written, the kernel selects the outcome
  `ess_conformance::interpret` selects, compared request by request.
- The same over a related row's absent `Optional` field read through `when_related:`.
- `an_undecidable_guard_names_its_absent_field`: where the interpreter determines no outcome, the
  kernel's `undecidable` refusal names the guard and the absent field and suggests
  `defined(<field>)`, for a held-row guard and for a `when_related:` guard; `run` answers it with the
  same status as today. At the base the message names neither, so this test is red first.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or **inferred** (a reading that could be wrong).

ESS paths below are relative to `crates/` of the ESS 0.99.1 source (tag 0.99.1, `Cargo.lock` rev `d7833ee4`). Spindle paths are relative to the worktree root.

- **Primary surface:** `crates/spindle-kernel` — cited (the story names `facts.rs`, and the refusal is built in `execute.rs`).
- **Files:**
  - `crates/spindle-kernel/src/execute.rs:732-737, 1069-1076, 1453` — cited. These are the guard-Unknown refusal sites.
  - `crates/spindle-kernel/src/lib.rs:171-178, 228-233` — cited. This builds the `Undecidable` message.
  - A new test module `crates/spindle-kernel/tests/spindle-kernel/<new>.rs` plus its `mod` line in `main.rs`, with a fixture under `crates/spindle-kernel/tests/fixtures/<new>/` — inferred. This follows the pattern of `absent_optional_guard.rs` and `absent_optional.rs`.
- **Symbols:** `Run::holds`, `Run::truth`, `Run::undecidable`, `ExecuteError::Undecidable { outcome, why }`, `Unknowns::read`/`Unknowns::why` (`execute.rs:466-512`, which already names an absent `Optional` field and suggests `defined(<field>)`, for row sets only), and `lib.rs::undecidable` — cited.
- **Also likely:** none.
  - `crates/spindle-kernel/src/facts.rs` is read, not changed — inferred. Its binding already matches ESS (see below).
  - `crates/spindle-telemetry` keeps the same error code `undecidable` (`lib.rs:242`), so only the message text changes — inferred.
- **Documents:** none — inferred.
- **Not touched:**
  - `facts.rs` binding (`:128-132`, `:160-162`, `:290-301`) — inferred.
  - The row-set path `execute.rs:920-980` — cited. It already names the field.
  - The input-guard path covered by wave 25 (`absent_optional_guard.rs`) — cited.
  - `spindle-cli` `run` — inferred. The story says it answers as today.
- **Confidence:** high for where the work lands; the ESS evaluation and the Spindle refusal sites were both read line by line. Medium on whether acceptance 1-2 need any kernel change: by reading, they already hold, but no test was run.
- **Would collide with:** any unit touching `spindle-kernel`'s guard selection and refusal (`execute.rs` `Run::holds`/`truth`/`undecidable`, `Unknowns`), the `ExecuteError::Undecidable` shape or message in `lib.rs`, or the kernel test root `tests/spindle-kernel/main.rs`.

**ESS semantics** (an `Optional` field of the held row is absent; the interpreter's fact source gives `observe` = None and `observed_presence` = Some(false)):

- **How the row is bound.**
  - An absent field projects no fact: `verify/ess-conformance/src/interpret/execute/history/facts.rs:50-60`.
  - `fact` reads only what was projected: `:261-263`.
  - Presence is `Some(false)` for a missing key, `Absent` or `null`: `:267-277` and `history/unrecorded.rs:77`.
  - An operand of a fact reads `observe`, which is `fact`: `specify/ess-primitives/src/predicate.rs:1100-1102` and `specify/ess-primitives/src/facts.rs:1564-1577`.
- **`field == x` and `field != x`:** Unknown. The operand does not resolve, so the comparison returns before it compares anything (`predicate.rs:2241-2244`). The enum form `when_subject: {field, equals}` takes the same path (`interpret/execute/subject.rs:111-119`).
- **Ordering (`<`, `<=`, `>`, `>=`):** Unknown, at the same early return (`predicate.rs:2241-2244`).
- **`field in [..]`, `none_of`/`not_in` (`AnyOf`/`NoneOf`):** Unknown (`predicate.rs:2186-2195`).
- **A bare boolean `field`:** Unknown (`predicate.rs:2180-2182`).
- **`defined(field)`:** False (`predicate.rs:2183-2185`).
- **`not (...)`:** Kleene negation (`predicate.rs:2173`, `:168-174`).
  - `not (field == x)` is Unknown.
  - `not defined(field)` is True.
- **Combining guards:** False wins in `all`, True wins in `any` (`predicate.rs:2167-2172`, `:147-163`).
  - `all: [defined(f), f == x]` is False.
  - `any: [not defined(f), f == x]` is True.
  - `coalesce(f, d)` (ess/29) reads `d` (`predicate.rs:720-731`).
- **What an Unknown does to the command:**
  - An Unknown held-row condition is `Undetermined::Undecidable { outcome, guard }` (`subject.rs:125-131`).
  - Selection stops there and never falls through to the default (`interpret/execute.rs:1298-1299`, `:1376-1382`).
  - ESS's own design doc says the same: `docs/design/cross-record-and-stored-field-guards.md:185-188` ("an unknown fact selects no branch and never the default … writes `not defined(field)`").
- **`when_related` rows:** they go through the same `Held::new` (`interpret/execute/related.rs:234`) and the same `Held::holds` (`subject.rs:90-101`), so the results are identical.
- **Where ESS enumerates instead:** only for an unrecorded generated value (`Value::Unknown`), never for an absent one (`history/facts.rs:73-105`).

**Gap confirmed:** partly. Only the refusal message is a confirmed gap; the claimed difference in how guards are decided was not found.

- **Deciding the guard: no divergence found** (inferred, not run).
  - Spindle binds an absent or `null` field as no fact with presence false (`crates/spindle-kernel/src/facts.rs:12-13`, `:160-162`, `:299-301`). This is the same as ESS.
  - It evaluates with the same `Predicate::evaluate`: `execute.rs:757` for `when_subject`, `execute.rs:766-767` for the enum `{field, equals}` (absent gives Unknown).
  - The related row takes the same path: `execute.rs:817-821` reads it through `related_facts` (`:1197-1207`), which calls `Facts::row`.
  - Spindle turns Unknown into `Undecidable` (`:736`, `:1453`), as ESS does. So the three guards the consumers reported are very likely undecidable in ESS 0.99.1 too, unless they were written with `defined(...)` or `coalesce(...)`.
- **The refusal message: gap confirmed** (cited).
  - For held-row and `when_related` guards, `Run::undecidable` gets a `why` only from the row-set `open` slot (`execute.rs:1069-1076`). Otherwise the message reads "…is undecidable over the held instance and the input" (`lib.rs:228-233`).
  - That message names neither the guard nor the absent field, and does not suggest `defined(<field>)`.
  - Acceptance 3 is that change, and it can reuse `Unknowns::read`/`why` (`execute.rs:466-512`).
  - Acceptance 1-2 are parity tests against `ess_conformance::interpret` that should pass at HEAD. One that fails would be the finding.

Not established: whether the consumers' guards actually use a bare comparison rather than `defined(...)`. I did not see their specifications.

## Decided when scoped

- ESS 0.99.1 answers Unknown for a comparison, ordering, `in`, `none_of` or a bare boolean over an
  absent `Optional` field of a held or related row, and selects no outcome, so such a guard is
  undecidable in ESS too. Acceptance 1 and 2 are parity tests against `ess_conformance::interpret`;
  each is expected to pass at the start, and one that fails is the defect this story fixes.
- The confirmed gap is acceptance 3: the `Undecidable` refusal of a held-row or `when_related`
  guard names the guard and the absent field and suggests `defined(<field>)`, reusing
  `Unknowns::read` and `Unknowns::why` (`execute.rs:466-512`). The error code stays `undecidable`.

## Shared files

- `crates/spindle-kernel/src/execute.rs` (`Run::undecidable`) and `lib.rs`: shared with
  `story:related-selector-rows-quantifier` in the same unit (U1), built after it on one branch.
- `crates/spindle-cli/tests/spindle-cli/absent_optional_guard.rs:361` asserts the old refusal text;
  this story updates that assertion. No other wave-26 unit edits that file.
- `CHANGELOG.md` `[Unreleased]`: one bullet per story, merged by the coordinator.

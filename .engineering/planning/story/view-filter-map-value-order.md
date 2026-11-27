---
format: aep.planning-md/3
id: story:view-filter-map-value-order
kind: story
status: draft
title: A view filter orders a Map value by its declared type
relations:
- serves: vision:spindle
- informed_by: story:map-input-guard-facts
- depends_on: story:view-filter-nested-element-quantifier
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-host/src/view.rs
- confidence: inferred
  path: crates/spindle-host/tests/fixtures/view-filter-map
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/main.rs
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/view_filter_map.rs
revision: 6
---
## Outcome

A view filter whose quantifier reads a value inside a `Map` (`Map<String, Timestamp>`, a newtype
over `Timestamp` as the value) is answered, and orders the value by its declared type, as ESS
0.99.1's interpreter does. Today Spindle refuses such a view whole: the `Exists`/`Forall` arm of
`Scope::admits` (`crates/spindle-host/src/view.rs:418-455`) admits only a `List` target, so the
read fails with `HostError::Unsupported` where ESS answers. Once admitted, a filter path is typed
with `resolve_path` only (`view.rs:189`, `:688`), which handles a list position and not a map one,
so the value must be typed at its ordinal as ESS's `map_value` does, or it compares as text.

Found by the wave 27 U4 implementor while fixing the same gap in the kernel's `Facts::declared`.

## Acceptance

- `a_view_filter_orders_a_map_value_by_its_instant`: a view filter with `exists` over a
  `Map<String, Timestamp>` field answers the rows ESS's interpreter answers, for an instant written
  with an offset that sorts differently as text.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Stale claim:** the Outcome says such a value "may compare as text by its bytes". At head
  394adc71 it cannot reach evaluation. The `Exists`/`Forall` arm of `Scope::admits`
  (`crates/spindle-host/src/view.rs:418-455`) refuses any quantifier whose target's
  `resolution.terminal` is not `ResolvedTypeRef::List` (`:436-440`). It would also fail
  `self.resolve(fields, "<map>.0")`, because ESS's `resolve_path` has no map ordinal
  (`ess-domain/src/expression.rs:1143-1155` at 0.99.1). `unsupported` (`view.rs:76-100`) therefore
  names the filter, `readable` (`crates/spindle-host/src/host.rs:2606`) returns
  `HostError::Unsupported`, and the read is refused whole. The defect today is that ESS answers and
  Spindle refuses. The byte order at `:189`/`:688` is a hazard that opening the arm would expose — cited
- **ESS 0.99.1 (parity target):** the checker accepts a quantifier over a `Map`
  (`ess-domain/src/expression.rs:3013`, `Shape::List(element) | Shape::Map(element)`). The
  interpreter binds a source row with `input::bind`, so a Map becomes `<path>.count` plus its values
  at `<path>.<index>` in key order (`ess-conformance/src/input.rs:1812-1818`). `ViewFacts`
  (`interpret/views.rs:569-576`) orders through `TypedFacts::orders_as_instant`
  (`input.rs:169-175`), whose `declared_primitive` (`input.rs:193-232`) types a map value at an
  ordinal through `map_value` (`:235-251`). So ESS orders a `Timestamp` map value by its instant — cited
- **Primary surface:** `crates/spindle-host` (view filter admission and evaluation) — cited
- **Files:** `crates/spindle-host/src/view.rs:418-455`: the `Exists`/`Forall` arm of
  `Scope::admits` also admits a `Map` terminal (through `Optional`), with the binder's element at
  `<map>.0` — cited
- **Files:** `crates/spindle-host/src/view.rs:184-190` (`Scope::resolve`) and `:685-696`
  (`Read::declared`): where `resolve_path` fails, fall back through a map ordinal as ESS's
  `declared_primitive` does: resolve the prefix, take its `map_value`, and resolve `value.<rest>`
  over one synthetic field. `Scope::resolve` needs the whole `Resolution`, not only the primitive,
  because `fits` and `ordered` read its `scalar` — cited
- **Files:** `crates/spindle-host/src/view.rs:65-70`, `:86-98`, `:134-139`: the doc of
  `unsupported`, its refusal text and the doc of `reads` say "a list parameter or a list field"; add
  a map field — cited
- **Symbols:** `Scope::admits`, `Scope::resolve`, `Scope::typed`, `Scope::element_member`,
  `Read::declared`, `Binder`, `unsupported`, `reads` — cited
- **Not touched:** the row binding. `bound`/`navigated` (`view.rs:921-971`) already bind through
  `ess_conformance::input::bind`, so the Map's count, index and key order are ESS's own — cited
- **Also likely:** `crates/spindle-host/tests/spindle-host/view_filter_map.rs`, a new module with
  its `mod` line in `crates/spindle-host/tests/spindle-host/main.rs`. It would be built like
  `always_and_key_filters.rs`: a `Host` on the memory and the SQLite store against
  `Interpreted::for_model`. Test `a_view_filter_orders_a_map_value_by_its_instant`: the filter
  `exists v in stamps: v > "2026-01-01T09:00:00Z"` over a `Map<String, Stamp>` field, where `Stamp`
  is a newtype over `Timestamp`, with one row holding `"2026-01-01T10:00:00+02:00"`. That row is
  true as text and false as an instant. Plus `forall`. Each test is a few seconds, well under the
  60 s bound; the test step stays under 120 s — inferred
- **Also likely:** `crates/spindle-host/tests/fixtures/view-filter-map/` (`system.yaml`,
  `domains/`): one component whose entity has a `Map<String, Stamp>` field, a command that writes
  it, and the view — inferred
- **Also likely:** `CHANGELOG.md` `[Unreleased]` `### Fixed`, one bullet — inferred
- **Documents:** the doc comments in `view.rs` named above; nothing else — cited
- **Confidence:** high for the defect site and the ESS rule, both read at the pinned rev; medium for
  the ordering test, because whether ESS stores a `Timestamp` with a non-`Z` offset unchanged was
  not read
- **Would collide with:** any unit that changes view-filter admission in `spindle-host`
  (`view.rs`: `Scope`, `Binder`, the `Exists`/`Forall` arm, `Read::declared`); the module list of
  the `spindle-host` integration-test binary (`tests/spindle-host/main.rs`); and `CHANGELOG.md`
  `[Unreleased]`
- **Safety fact:** every view the arm admits today keeps its answer. The map fallback runs only
  where `resolve_path` fails on a path whose prefix resolves to a Map, and no filter admitted today
  reads a Map, because the arm refuses it (`view.rs:436-440`). The change only turns refused views
  into answered ones. Proven to step 3 (walked `admits` → `unsupported` → `readable`), unproven
- **Architecture:** none. No new crate, process, wire format or dependency — cited

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: a local helper in `view.rs` mirrors ESS's `map_value` and returns a `Resolution`.
- D2 A: only a `Map` field of the row is admitted, not a `Map` view parameter.
- D3 B: one unit with `story:view-filter-nested-element-quantifier`, after it.
- D4 A: the Outcome says Spindle refuses the view where ESS answers; once admitted, a map value
  is typed at its ordinal so a `Timestamp` orders by its instant.

---
format: aep.planning-md/3
id: story:view-filter-nested-element-quantifier
kind: story
status: draft
title: A view filter reads a quantifier over a member of an outer list element
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-host/src/view.rs
- confidence: cited
  path: crates/spindle-host/tests/spindle-host/adversary_w26_u2.rs
- confidence: inferred
  path: website/docs/concepts/specification.md
revision: 5
---
# A view filter reads a quantifier over a member of an outer list element

## Outcome

A view filter that quantifies over a list member of an outer list-field element, as in
`exists g in slots: (exists w in g.words: w == "x")`, is read as ESS 0.99.1's interpreter reads it.
Today Spindle refuses the view whole: the `Exists`/`Forall` arm of `Scope::admits`
(`crates/spindle-host/src/view.rs:441-455`) builds the inner element path from the written path
`g.words.0`, which names no field of the source, where the outer binder's element path
`slots.0.words.0` does. The interpreter answers the read.

## Acceptance

- `a_quantifier_over_an_outer_elements_list_member_agrees`: the filter above answers the
  interpreter's rows on the memory and the SQLite store, compared row by row.

## Origin

Adversary pass of wave 26 U2 (`story:view-filter-parameter-literal`), 2026-11-26, recorded as a
note: Spindle serves less than ESS there and never a different answer. Undecided whether it was
present at the base. Test `a_quantifier_nested_over_an_element_member_is_read_or_refused_whole` in
`crates/spindle-host/tests/spindle-host/adversary_w26_u2.rs` passes through the refusal branch today.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-host`, the filter admission of `src/view.rs` — cited
- **Files:** `crates/spindle-host/src/view.rs:418-455`, the `Exists`/`Forall` branch of `Scope::admits`; the element path is built from the written `over` path at `:428-434` and the view is refused at `:435-440`. The story's `:441-455` is the binder push and the recursion, so its range is off by the 23 lines above it — cited
- **Files:** `crates/spindle-host/tests/spindle-host/adversary_w26_u2.rs`: the new test `a_quantifier_over_an_outer_elements_list_member_agrees` goes here and calls `agree(&["org.bench.SlotWords"], &[])` (`:277-292`, memory store and SQLite, row by row against `ess_conformance::interpret`). The view and its rows already exist in `crates/spindle-host/tests/fixtures/adversary-w26-u2/domains/bench.yaml:183-191` with the script at `:41-52` (words `["x"]`, `[]`, `["y","x"]`, `["X"]`), so the fixture does not change — cited
- **Symbols:** `Scope::admits`, `Scope::typed` (`view.rs:195-266`, the `[root, members @ ..]` branch for a row binder at `:236-245`), `Scope::element_member` (`:301-325`), `Binder { name, element, row }` (`:156-160`). The fix: where `over` is `<row binder>.<members>`, the element path is `binder.element ++ members ++ "0"` over the source fields, with `row: true`. It is not built from `over`'s own segments — cited
- **Also likely:** the doc comment of `unsupported` and its refusal text (`view.rs:61-66`, `:88-98`) add "a list member of an outer list field's element" — inferred
- **Also likely:** `website/docs/concepts/specification.md:168-173` (the forms a filter may take) and `CHANGELOG.md` — inferred
- **Also likely:** the doc comment of the host's read method, `crates/spindle-host/src/host.rs:1974-1977`; wave 29 also changes `host.rs` — inferred
- **Documents:** the two inferred document lines above; the code change is in `view.rs` alone
- **Tests to add:** `a_quantifier_over_an_outer_elements_list_member_agrees` in `adversary_w26_u2.rs`: one view, every read of it (one, since the view has no parameters), 5 cards, memory and SQLite. Not timed; expected well under 5 s per test, within the 60 s per-test bound and the 120 s step budget — inferred
- **Confidence:** high — the story names the place that refuses the view, and the tree shows the one path build that refuses `g.words.0`
- **Would collide with:** any unit that edits `Scope::admits`, `Scope::typed` or `Scope::resolve` in `crates/spindle-host/src/view.rs` (a quantifier over a `Map`, how a filter path's type is resolved, a new filter form); any unit that edits `view.rs` elsewhere (only a text merge); `CHANGELOG.md`; `website/docs/concepts/specification.md`
- **Safety fact:** Spindle only decides whether to admit the filter; ESS's evaluator (`predicate.evaluate`, `view.rs:998`) decides which rows match. Its `Element` reads the inner binder through the outer one (ESS 0.99.1 `ess-primitives` `src/predicate.rs:1871-1895`), so `w` reads `slots.<n>.words.<m>`. `Read::source` (`view.rs:664-679`) reads that path from the bound row, because `slots` is no reference — step 3, unproven

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: the acceptance test goes in `adversary_w26_u2.rs`, reusing its fixture and `agree`.
- D2 B: one unit holds this story and `story:view-filter-map-value-order`, this one first.

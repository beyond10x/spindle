---
format: aep.planning-md/3
id: story:view-filter-nested-element-quantifier
kind: story
status: draft
title: A view filter reads a quantifier over a member of an outer list element
relations:
- serves: vision:spindle
revision: 1
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

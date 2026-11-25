---
format: aep.planning-md/3
id: story:view-filter-always-and-key
kind: story
status: draft
title: A view filter reads an (always) leaf and a key path
summary: An (always) leaf inside a quantifier and a key.<field> path in a view filter are read as ESS's interpreter reads them.
relations:
- serves: vision:spindle
- informed_by: story:view-field-literal-filters
- informed_by: story:views-with-filters-and-lists
- informed_by: story:view-filter-through-reference
revision: 1
---
# A view filter reads an (always) leaf and a key path

## Outcome

A view filter with an `(always)` leaf inside a quantifier, or a `key.<field>` path (the row's
identity key), is read as ESS's interpreter reads it, and the view answers the same rows, instead
of being refused whole.

## Why

A consumer's census: about 20 views use one of these constructs and are refused whole.

- a) An `(always)` leaf inside a quantifier:
  `(not (defined(param.tags)) or not (exists t in param.tags: (always)) or exists t in param.tags: (exists u in tags: (u == t)))`
- b) A `key.<field>` path: `key.integration == param.id`

The filter reader `reads` (`crates/spindle-host/src/view.rs:127`) admits neither leaf, so
`unsupported` refuses the view with the list of what is read (`view.rs:76-84`). The quantifier
over a list parameter and a list field is read already (`story:view-field-literal-filters`), as
are filters against parameters (`story:view-filter-through-reference`).

## Acceptance

- `an_always_leaf_in_a_quantifier_is_read`: the view with filter a) is served; with `param.tags`
  absent, empty, and naming a tag some rows hold, it answers the rows ESS's interpreter answers.
- `a_key_path_is_read`: the view with filter b) is served and answers the rows whose identity key
  `integration` equals the parameter, the rows ESS's interpreter answers.
- `a_view_with_both_is_served`: no view of the census refused for either construct is refused
  after this story.

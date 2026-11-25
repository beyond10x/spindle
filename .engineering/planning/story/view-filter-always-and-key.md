---
format: aep.planning-md/3
id: story:view-filter-always-and-key
kind: story
status: implemented
title: A view filter reads an (always) leaf and a key path
summary: An (always) leaf inside a quantifier and a key.<field> path in a view filter are read as ESS's interpreter reads them.
relations:
- serves: vision:spindle
- informed_by: story:view-field-literal-filters
- informed_by: story:views-with-filters-and-lists
- informed_by: story:view-filter-through-reference
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-docs/src/status.rs
- confidence: cited
  path: crates/spindle-host/src/view.rs
- confidence: cited
  path: crates/spindle-host/tests/fixtures/always-and-key
- confidence: cited
  path: crates/spindle-host/tests/spindle-host/always_and_key_filters.rs
- confidence: cited
  path: crates/spindle-ingress/tests/spindle-ingress/always_and_key_routes.rs
revision: 8
transitions:
- {from: "draft", to: "proposed", at: "2026-11-25T10:08:45Z", actor: "human:operator", revision: 6, decided_on: {"recorded":{"review_outcome":7}}}
- {from: "proposed", to: "active", at: "2026-11-25T10:08:45Z", actor: "human:operator", revision: 7, decided_on: {"recorded":{"review_outcome":7}}}
- {from: "active", to: "implemented", at: "2026-11-25T12:11:04Z", actor: "human:operator", revision: 8, decided_on: {"recorded":{"test_result":1,"review_outcome":7}}}
---
# A view filter reads an (always) leaf and a key path

## Outcome

A view filter with an `(always)` leaf inside a quantifier, or a `key.<field>` path (a member of
the row's struct identity), is read as ESS's interpreter reads it, and the view answers the same
rows, instead of being refused whole. Admitted besides what is read today:

- `always` and `never`, anywhere a leaf is read; ESS evaluates them to true and false for every
  row (`ess-primitives` `Predicate::evaluate` at ESS 0.99.1).
- A path `<field>.<member>…` whose field is a struct field of the source, the identity among them,
  each segment a member of the struct the one before it holds, through `Optional` (a newtype over
  a struct is not walked, and such a path stays refused). ESS reads it in the row, never through a
  reference (`ResolvedView::navigated` leaves a struct member to the struct), from the row
  `ess-conformance` binds (`input::bind`). It is read as a field is: compared with a literal, a
  parameter or another field, listed, `defined`.

Each is evaluated by ESS (`Predicate::evaluate`), never re-implemented. Any other leaf, and any
other path of two or more segments that is no struct member and navigates no reference (a list's
count such as `tags.count`, a text's length), is still refused whole.

### Deliberately wider than the request

The request names `(always)` inside a quantifier and `key.<field>` on the identity. The story
admits `never` and `always` everywhere a leaf is read, and the members of every struct field (the
identity is one), at any depth and through `Optional`, in every position a field is read. This is
deliberate: ESS defines each of them the same way in every position and for every struct field,
so admitting only the requested spelling would be a rule of Spindle's own that ESS does not have,
and would need code to tell the positions apart. Each extra is proved by a view of the fixture:
`Named` (`never`), `Listed` (`in:`), `Compared` (`defined`, a member compared with another field)
and `Regional` (`origin.place.region`, a struct field that is no identity, two levels deep,
through `Optional`).

## Why

A consumer's census: 8 routes and 193 journeys answer 501 because about 20 views use one of these
constructs and are refused whole.

- a) An `(always)` leaf inside a quantifier:
  `(not (defined(param.tags)) or not (exists t in param.tags: (always)) or exists t in param.tags: (exists u in tags: (u == t)))`
- b) A `key.<field>` path: `key.integration == param.id`

The filter reader `reads` (`crates/spindle-host/src/view.rs`) admits neither: `Scope::admits` has
no arm for `Predicate::Always`, and `Scope::typed` reads a two-segment path only through a
reference, so `unsupported` refuses the view with the list of what is read and the route answers
501. The quantifier over a list parameter and a list field is read already
(`story:view-field-literal-filters`), as are filters against parameters
(`story:view-filter-through-reference`).

## Shared files

- `crates/spindle-host/src/view.rs`: this story changes `unsupported`'s message, `Scope::admits`
  and `Scope::typed`. The drafts `story:cross-component-view-refused` (`unsupported`),
  `story:view-paging` (the paging refusal in `unsupported`), `story:view-read-families` and
  `story:view-rows-absent-optional-fields` (`project`) land on the same file. This story lands
  first, as the only one of them active; each of the drafts is rebased on it when it is taken up.
- `CHANGELOG.md` `[Unreleased]`: shared with the active `story:ess-099-path-types-and-wrapped-views`
  and `story:node-export-serves-a-specification`; the status page generated from
  `crates/spindle-docs/src/status.rs` with `story:ess-099-path-types-and-wrapped-views`. This
  story adds one bullet and one status item of its own and edits none of their lines; whichever
  merges later takes the other lines as they are.

## Acceptance

One fixture, `crates/spindle-host/tests/fixtures/always-and-key/`: links keyed by the struct
`{integration, name}`, each with a label, a struct `origin` holding an `Optional` struct `place`
with a `region`, and a `List<String>` of tags. Its views: `Tagged` (filter a), `OfIntegration`
(filter b), `OfIntegrationTagged` (both), `Named` (`never or key.name == "alpha"`), `Listed`
(`key.integration in [chat]`), `Compared` (`defined(key.name) and key.name == label`), `Regional`
(`origin.place.region == "eu"`), `Counted` (`tags.count == 0`) and `Folded` (`always and label
equals_ignore_case one`). Before: every view is refused by `spindle_host::reads` naming its
filter. Scenarios in the spindle-host test binary, module `always_and_key_filters`:

- `an_always_leaf_in_a_quantifier_is_read`: `Tagged` is served; with `tags` absent, empty, naming
  a tag some links hold and naming one none holds, it answers the rows ESS's interpreter answers.
- `a_key_path_is_read`: `OfIntegration` is served and answers the links whose key's
  `integration` equals the parameter, the rows ESS's interpreter answers; `Named`, `Listed`,
  `Compared` and `Regional` are served and answer the interpreter's rows.
- `a_view_with_both_is_served`: `OfIntegrationTagged` is served and answers the interpreter's
  rows.
- `a_path_no_struct_holds_is_still_refused`: `Counted` stays refused by `spindle_host::reads`,
  the refusal naming the view and `tags.count`.
- `an_unread_leaf_beside_always_is_still_refused`: `Folded` stays refused, the refusal naming
  the view and `equals_ignore_case`.
- `always_and_key_filters_agree_with_the_interpreter`: on the memory and SQLite stores, on the
  host's thread and through its reader, these reads answer the interpreter's rows: `Tagged` with
  `tags` left out, `null`, `[]`, `[red]`, `[blue, red]` and `[green]`; `OfIntegrationTagged` with
  each of those and `id` `mail`, `chat` and `none`; `OfIntegration` with `id` `mail`, `chat` and
  `none`; `Named`, `Listed`, `Compared` and `Regional` with no parameter. Every view of the
  fixture but `Counted` and `Folded` is served.

End to end, in the spindle-ingress test binary, module `always_and_key_routes`:
`always_and_key_routes_answer_the_interpreters_rows`: each served view's route, at the path ESS
derives, answers `200` with a body its projected schema admits and the rows ESS's interpreter
answers; before, 501.

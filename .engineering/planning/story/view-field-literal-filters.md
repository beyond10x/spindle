---
format: aep.planning-md/3
id: story:view-field-literal-filters
kind: story
status: implemented
title: A view's filter reads a field compared with a literal, and a quantifier over a list field
relations:
- serves: vision:spindle
- depends_on: story:view-list-query-parameters
scope:
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/view-field-filters
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/view_field_filters.rs
- confidence: cited
  path: crates/spindle-host/src/view.rs
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-07T12:45:25Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-07T12:45:25Z", actor: "human:timo", revision: 5}
- {from: "active", to: "implemented", at: "2026-11-07T17:09:21Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":1,"review_outcome":1,"verification":1}}}
---
# A view's filter reads a field compared with a literal, and a quantifier over a list field

## Outcome

A view whose filter compares a field of its source with a literal, or quantifies over a list field
of the row, is answered with the rows ESS's interpreter answers. Admitted besides what
`story:view-list-query-parameters` admits:

- an observable field of the source compared with a literal of its type by `==`, `!=`, `<`, `<=`,
  `>` or `>=`, the ordered operators only where ESS orders that type (`archived == false`,
  `priority >= 3`, `kind != "draft"`);
- an observable field listed `in:` or `none_of:` a list of literals of its type;
- `exists` or `forall` over a list field of the row (`in: tags`), whose body reads the element
  under its binder as a field is read above, or compared with a parameter or an outer binder.

Each leaf is evaluated as ESS evaluates it (`ess_primitives::predicate`), never re-implemented; an
absent optional field is decided as ESS decides it. Any other leaf is still refused whole with its
text. Today `crates/spindle-host/src/view.rs` (`unsupported`, `admitted`) refuses both, and
`crates/spindle-ingress/src/answer.rs` answers the route 501.

## Acceptance

One neutral fixture, `crates/spindle-cli/tests/fixtures/view-field-filters/`: an entity with a
Boolean, an Integer, a String enum-like field, an optional field and a `List<String>` field, rows
covering each case, and one view per construct. Before: each route answers 501 naming the filter.
Scenarios in the spindle-cli test binary, module `view_field_filters`:

- `a_field_compared_with_a_literal_filters_rows`: `==`, `!=` and an ordered comparison each
  answer exactly the rows ESS's interpreter answers.
- `a_field_listed_among_literals_filters_rows`: `in:` and `none_of:` over a field.
- `an_absent_optional_field_is_decided_as_ess_decides_it`.
- `a_quantifier_over_a_list_field_filters_rows`: `exists` and `forall` over the list field, the
  body comparing the element with a literal and with a parameter.
- `an_unread_filter_is_still_refused_whole`: a filter mixing an admitted leaf with one still
  unread answers 501 naming the filter.
- `field_filters_agree_with_the_interpreter`: for every view of the fixture, the rows equal those
  ESS's interpreter answers over the same steps.

## Scope

- `crates/spindle-host/src/view.rs` — cited: `unsupported`, `admitted`, the row evaluation.
- `crates/spindle-cli/tests/spindle-cli/view_field_filters.rs` — inferred, new.
- `crates/spindle-cli/tests/fixtures/view-field-filters/` — inferred, new.

## Origin

A consumer serving a specification as a UI test backend at 0.5.0: views filtering on a field
literal and quantifying over a list field answer 501. Follows `story:view-list-query-parameters`.

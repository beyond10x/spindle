---
format: aep.planning-md/3
id: story:view-filter-parameter-literal
kind: story
status: draft
title: A view filter reads every parameter form a consumer's model declares
relations:
- serves: vision:spindle
- informed_by: story:view-filter-text-contains
revision: 3
---
# A view filter reads every parameter form a consumer's model declares

## Outcome

A view filter that reads a parameter in any of the forms below, combined by `any`, `all` and `not`
with the leaves read today, is read as ESS 0.99.1's interpreter reads it, and the view answers the
same rows instead of being refused whole (`crates/spindle-host/src/view.rs`, `unsupported`,
`reads`):

- a parameter compared with a literal of its type: `param.label == ""`, `param.has_agent == false`,
  `param.type == all`;
- a text parameter under a text operation: `param.open in_ignore_case ["on", "true"]`,
  `p equals_ignore_case "inbound"` for an element `p` of a list parameter, `trimmed(...)` around
  such a test;
- a parameter cast in a comparison: `created >= param.time_start as timestamp`;
- the count of a list parameter: `param.type.count == 0`;
- a member of an element of a list field compared with a parameter:
  `exists g in groups: (g.id == param.id)`.

Reported 2026-11-26 by a consumer on Spindle 0.21.0: 205 of its journeys stop at a list route
answering `501` with `unsupported: view … declares the filter …`, on nine routes, and seven more
routes answer the same with no journey reaching them first. Each filter there is built of the forms
above and of leaves Spindle already reads.

## Acceptance

- One test per form above: a view whose filter holds that form answers the interpreter's rows for
  the parameter left out and given, compared with `ess_conformance::interpret` row by row.
- `the_consumer_filter_shapes_are_served`: three views built of the forms together, in the shape of
  the consumer's filters (optional-parameter guards `not defined(param.x) or …`, an `in_ignore_case`
  switch over a state, a time window cast `as timestamp`, a list-parameter `count == 0` escape),
  are served by `admit` and `run` and answer the interpreter's rows; none answers `501`.
- A form outside this list stays refused whole, naming the filter.

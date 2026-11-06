---
format: aep.planning-md/3
id: story:view-list-query-parameters
kind: story
status: active
title: A view's list parameter is read from its repeated query key
relations:
- depends_on: story:one-test-binary-per-crate
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/view-list-parameter
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T15:43:16Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"review_outcome":1}}}
- {from: "proposed", to: "active", at: "2026-11-06T15:43:16Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"review_outcome":1}}}
---
# A view's list parameter is read from its repeated query key

## Outcome

A view parameter of type `List<T>` that the route does not carry in its path is read from the
query string as ESS publishes it: the key repeated once per element (`tags=a&tags=b`, OpenAPI
`style: form`, `explode: true`; ESS 0.88.0 `crates/generate/ess-gen/src/openapi.rs:3369-3412`,
beyond10x/ess#438), each element read as `T`, required unless the parameter is `Optional`, and the
view's filter reads the list as `param.<name>`. The comma form `tags=a,b` is not read as a list,
because ESS does not publish it. Today `crates/spindle-ingress/src/surface.rs:889` refuses the view
as "the parameter `<name>`, which no single query value carries".

## Acceptance

One neutral fixture, `crates/spindle-cli/tests/fixtures/view-list-parameter/`: view `Tagged` with a
required `tags: List<String>` its filter reads with membership, view `Numbered` with a required
`n: List<Integer>`, and view `MaybeTagged` with `tags: Optional<List<String>>`, beside rows holding
several values. (The fixture `list-parameter/` belongs to the startup fix in wave 10 and is not
this story's.) Before: each route answers 501 naming the parameter.

- `a_repeated_query_key_is_read_as_a_list`: `Tagged?tags=a&tags=b` answers exactly the rows whose
  value is `a` or `b`, as the rows ESS's interpreter answers for that parameter.
- `a_single_key_is_a_list_of_one`: `Tagged?tags=a` answers the rows of `a`.
- `a_missing_required_list_parameter_is_refused_400`: no `tags` key answers 400 naming the
  parameter, as a missing required scalar parameter does (`story:ingress-view-parameters`).
- `an_element_of_the_wrong_type_is_refused_400`: `Numbered?n=1&n=x` answers 400 naming `n`.
- `an_empty_element_is_refused_400`: `Tagged?tags=` answers 400 naming `tags`, as the scalar path
  refuses empty text (`crates/spindle-ingress/src/surface.rs:177-178`).
- `a_missing_optional_list_parameter_answers_unfiltered`: `MaybeTagged` with no `tags` key answers
  the rows ESS's interpreter answers for an absent parameter.

## Order

- `depends_on` `story:one-test-binary-per-crate`: the scenarios are one module,
  `view_list_parameter`, in the spindle-cli test binary that story builds.
- Shares `crates/spindle-ingress/src/surface.rs` with active `story:outcome-scoped-refusal`,
  `story:http-outcome-bodies`, `story:ingress-request-log` and `story:live-topology`; scope keeps
  them in separate waves.

## Origin

A consumer serving a specification as a UI test backend (2026-11-06): 17 views of its
specification have a list-typed parameter; three routes taking a list of tags carry 64 calls in
one UI run. Follows `story:ingress-view-parameters`.

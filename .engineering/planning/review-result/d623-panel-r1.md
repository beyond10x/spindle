---
format: aep.planning-md/3
id: review-result:d623-panel-r1
kind: review-result
status: active
title: D623 story, plan-critic panel, round 1
relations:
- reviews: story:view-list-query-parameters
revision: 1
---
needs-revision

Panel of four lanes on story:view-list-query-parameters, run by one agent. Verdicts: acceptance needs-revision; design, parallel-safety and scope approve.

LANE: acceptance — needs-revision
- Gap 1, no scenario for the optional case. The Outcome says each list parameter is "required unless the parameter is `Optional`", but no scenario checks that a missing `Optional<List<T>>` key gets the unfiltered answer instead of a 400. Only the required case is covered (`a_missing_required_list_parameter_is_refused_400`).
- Gap 2, the fixture can't run the integer scenario. The fixture is described as "a view with a `List<String>` parameter". `an_element_of_the_wrong_type_is_refused_400` needs a `List<Integer>` parameter `n`, so the fixture would need a second view, or a second parameter, that it doesn't declare.
- Note: empty element is undefined. Nothing says what `?tags=` (one empty element) should do. Today's scalar path rejects empty text with "expected a value, found empty text" (`crates/spindle-ingress/src/surface.rs:177-178`).
- The other scenarios are fine: each has a before state (501 naming the parameter) and an observable after state, and each is checked against what ESS's interpreter answers.
- Read: the story (rev 2) and `surface.rs:153-182` and `:870-900`. Could not establish whether ESS's interpreter accepts an absent optional list.

LANE: design — approve
- Where the change lands: `Route::parameters` (`surface.rs:153`), which refuses more than one value ("expected one value, found {}" at :167-171), and the route builder's `Shape::One | Shape::Optional` refusal at :885-891. `Queried.flat` (:28-34) will need the parameter's shape as well.
- Scope looks wrong on `answer.rs`: it only calls `route.parameters(query)` at :135.
- A fixture already exists in the code tree: an untracked `crates/spindle-cli/tests/fixtures/list-parameter/` with view `NotesTagged` (`tags: List<String>`, an `exists/in` filter), unreferenced; settle the name or ownership.
- No dependency cycle: the only incoming edge is `depends_on story:one-test-binary-per-crate`, which is active.

LANE: parallel-safety — approve
- The Order section names the overlap on `surface.rs` with the four active stories and relies on scope to keep them in separate waves.
- All three scope entries are `inferred`, not cited.
- The test-binary dependency is real and declared.
- Read: `aep plan artifact list/show/waves`. Did not check per-wave placement of the four active stories.

LANE: scope — approve
- The comma form is excluded, with a source: ESS 0.88.0 `ess-gen/src/openapi.rs:3369-3412` sets `style: form`, `explode: true` for list parameters.
- No binding-level style setting exists under `crates/specify`, so "as the binding declares" can only mean the repeated key.
- Note for the consumer: comma-form calls stay unsupported unless ESS publishes that form.
- Nothing in the story goes beyond the request.

```findings
[{"file":".engineering/planning/story/view-list-query-parameters.md","category":"acceptance","severity":"blocker","message":"Outcome promises an Optional List parameter may be absent; no scenario checks it"},
 {"file":".engineering/planning/story/view-list-query-parameters.md","category":"acceptance","severity":"blocker","message":"Fixture is described as one List<String> view, but scenario an_element_of_the_wrong_type_is_refused_400 needs a List<Integer> parameter n; declare it in the fixture"},
 {"file":"crates/spindle-ingress/src/surface.rs","line":177,"category":"acceptance","severity":"note","message":"Behaviour of an empty element (?tags=) is not stated; the scalar path refuses empty text"},
 {"file":"crates/spindle-ingress/src/answer.rs","line":135,"category":"design","severity":"note","message":"answer.rs only calls Route::parameters; the change lands in surface.rs. The inferred scope entry is likely spurious"},
 {"file":"crates/spindle-cli/tests/fixtures/list-parameter/domains/pad.yaml","line":55,"category":"design","severity":"warning","message":"An untracked, unreferenced fixture with a List<String> view parameter exists in the wave/10 tree under another name than the story's view-list-parameter; settle which one the story owns"},
 {"file":".engineering/planning/story/view-list-query-parameters.md","category":"parallel-safety","severity":"warning","message":"Separation from four active stories on surface.rs rests on inferred scope only; cite the scope so the wave planner's overlap check is reliable"},
 {"file":".engineering/planning/story/view-list-query-parameters.md","category":"scope","severity":"note","message":"Comma form excluded with an ESS citation; the consumer should be told comma-form calls stay unsupported unless ESS publishes that form"}]
```

---
format: aep.planning-md/3
id: review-result:wave30-adversary-U10-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 30 U10
relations:
- reviews: story:related-via-nested-input
revision: 1
---
```
unit: story:related-via-nested-input, head ca5dc368 (my tests committed on top as 26e2628e)
verdict: nothing found
cases: executed 16→24, red 0
origin: introduced 0 / pre-existing 0 / undecided 0
wrote-outside-worktree: 1 path
needs-coordinator: none
```

I found nothing to break. ESS 0.99.2 refuses at load every nested shape I tried, so no specification with one reaches the kernel's flat read in `Run::reference` (`crates/spindle-kernel/src/execute.rs:985`). The unit's test fails as soon as ESS accepts the guard, so it can fail.

**1. Diff stat** (`git --no-pager diff --stat ca5dc368 HEAD`). Both paths are test files; no implementation file changed.
```
 .../tests/spindle-spec/adversary_w30_u10.rs        | 392 +++++++++++++++++++++
 crates/spindle-spec/tests/spindle-spec/main.rs     |   1 +
```

**2. Cases added** in `<local path>`, all green now. Each one first loads the same specification with the row named through a top-level field, so a refusal comes from the nested name and not from the rest of the specification.

| Case | Shape | What it asserts |
|---|---|---|
| `an_optional_struct_member_is_refused` | `wrap: Optional<Wrap>`, `via: input.wrap.shelf` | `[undeclared_reference]` names `` `wrap.shelf` `` |
| `a_nested_member_at_the_first_format_of_input_paths_is_refused` | the same guard at format ess/22 | same |
| `a_nested_member_beside_a_top_level_row_is_refused_in_either_order` | the several-rows path (`validate_several`), nested row first and second | same |
| `a_nested_member_in_a_predicate_guard_is_refused` | `when_related: {via: input.wrap.shelf, predicate: …}` | same |
| `a_nested_member_in_an_all_branch_is_refused` | the second row of an `all:` branch | same |
| `a_member_of_a_list_of_structs_is_refused` | `wraps: List<Wrap>`, `via: input.wraps.shelf` | `` `wraps.shelf` `` |
| `a_value_source_through_a_nested_member_is_refused` | `{related: {via: input.wrap.shelf, field: capacity}}` in a payload | `[undeclared_reference]` at `…BookPlaced.capacity` |
| `a_member_of_a_stored_struct_is_refused` | bare `via: wrap.shelf` into a struct the subject stores (the kernel's flat read of a subject field has the same shape) | a refusal line naming `MoveBook`, `outcomes.no-shelf.when_related` and `wrap.shelf` |

On the first run of the module alone (`cargo test --locked -p spindle-spec --test spindle-spec adversary_w30_u10`) one case was red. That was my own assertion being too strict, not a defect: ESS refused the value source but names the site, not the path:
```
[undeclared_reference] command.demo.shelf.PlaceBook.outcomes.placed.payload.demo.shelf.BookPlaced.capacity: `{related: …}` names one field in `via:` (…) and one field in `field:`
```
I changed the assertion to the site and the rerun gave `test result: ok. 8 passed; 0 failed; … 16 filtered out; finished in 0.01s`. The 16 deselected cases are where the "before" count of 16 comes from. Every new case takes about 0.01 s.

**3. Suite run** (`cargo test --locked -p spindle-spec --no-fail-fast`), exit 0:
```
test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```
- `cargo clippy --locked -p spindle-spec --all-targets -- -D warnings`: exit 0.
- `cargo fmt -p spindle-spec --check`: clean, after `slot --fetch crates/spindle-spec cargo fmt -p spindle-spec`.

**4. Judgement findings:** none.

**5. Attacked and not broken**
- Every nested input via in ESS goes through an exact `command.input_field` lookup (`related_guard.rs:1672`, `value_expression.rs:678`, `related_list.rs:39`, `validate_several` at `related_guard.rs:2081`), so none gets past the refusal.
- Nothing builds the IR other than `spindle_spec::compile`: `spindle-embed` calls it too, and `model_json` is only ever written, never read back.
- The kernel's value sources already follow dotted paths (`values.rs:620` → `read_input`), so they are not affected.
- Not covered by a test: whether a row-set `where:` that reads a nested input gives the same answer in the kernel and in ESS's interpreter. From reading the code both follow the dotted path (kernel `borrowed_input` at `values.rs:873`), but I did not run it.

**6. Paths written outside the worktree**
- `<local path>`

I took and released my own worktree lease (`adv-w30-u10`).

```findings
[]
```

---
format: aep.planning-md/3
id: review-result:wave30-adversary-U2-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 30 U2
relations:
- reviews: story:view-rows-absent-optional-fields
revision: 1
---
```
unit: story:view-rows-absent-optional-fields (wave 30 U2), findings cover head b0b233f1 (unit head ccf4e518 plus my test commit)
verdict: nothing found
cases: executed 107→118, red 0
origin: introduced 0 / pre-existing 0 / undecided 0
wrote-outside-worktree: 11 paths, all in the assigned adv-scratch directory (part 6)
needs-coordinator: none
```

Nothing broke. The change matches ESS 0.99.2's interpreter on every shape you named, and served views did not change.

**1. Diff of my changes** (`git --no-pager diff ccf4e518 --stat`). Every path is a test or a fixture:
```
 .../fixtures/adversary-w30-u2/domains/items.yaml   | 191 ++++++++++
 .../tests/fixtures/adversary-w30-u2/system.yaml    |  21 ++
 .../tests/spindle-conformance/adversary_w30_u2.rs  | 388 +++++++++++++++++++++
 .../tests/spindle-conformance/main.rs              |   1 +
```
Committed as b0b233f1 through `b10x-gates bot -- commit`. The privacy grep printed nothing.

**2. Cases added** in `crates/spindle-conformance/tests/spindle-conformance/adversary_w30_u2.rs`, fixture `tests/fixtures/adversary-w30-u2`. Each case runs the same steps on `HostTarget::query_view` and on `Interpreted::for_model` and compares rows (and `total`) exactly. All 11 are green at this head.

I also ran them against the base 8eaea47a in a managed worktree, which I removed afterwards: 9 were red there and 2 green (the served view and the aggregate). So the cases do see the change, and they show the served path and the aggregate skip did not move.

| Case | What it covers |
|---|---|
| `an_unwritten_optional_list_and_struct_read_as_ess_reads_them` | Optional list, Optional struct, Optional integer and reference reads all `null`; also checks ESS's row against a literal |
| `a_struct_written_without_its_optional_member_reads_as_ess_reads_it` | a struct field written without its optional member |
| `a_read_into_a_row_without_its_omitted_when_absent_field_reads_as_ess_reads_it` | reads through a reference: the far row lacks an `omitted_when_absent` field, holds a list, or the reference names no row; includes a read of the far row's identity |
| `a_read_into_a_row_holding_null_reads_as_ess_reads_it` | far row set up through `establish_entity` holding `null` in an `omitted_when_absent` field |
| `an_ordered_view_ranks_unwritten_fields_as_ess_ranks_them` | `order_by: [rank desc]` over rows that have no rank |
| `a_filtered_view_with_a_parameter_reads_as_ess_reads_it` | a filter with a parameter, two parameter values |
| `presence_declared_on_the_row_shape_reads_as_ess_reads_it` | a `shape:` view whose struct declares `omitted_when_absent` and `null_when_absent` that the entity's fields don't have |
| `renamed_and_computed_fields_read_as_ess_reads_them` | `wire:` renames, a computed field, a renamed reference read |
| `an_aggregate_grouped_by_an_unwritten_key_reads_as_ess_reads_it` | `group_by` on a field some rows never wrote |
| `a_paged_view_reads_as_ess_reads_it` | paging with and without page/size, `total`, an empty list beside a missing one |
| `a_served_view_still_leaves_every_unwritten_field_out` | `Host::listed_with` checked against literals: unwritten fields left out, `null` only for `null_when_absent` and computed fields |

No case is red, so there is no red output to quote. While I wrote the fixture, the loader refused three versions of it; these were fixture errors, not findings:
- `presence:` on an inline view field: `unknown field`.
- paging parameters not declared: `undeclared_reference`.
- `state` typed `Optional<String>`: `type_mismatch`.

**3. Suite run** (after the cases existed):
- `cargo test --locked -p spindle-conformance --no-fail-fast`: exit 0, `118 passed; 0 failed; 3 ignored`, 17 s wall.
- Same run with my module excluded: 107 passed (the `<before>` count).
- My cases alone: `11 passed; 110 filtered out; finished in 0.01s`. None is over 5 s.
- `cargo fmt -p spindle-conformance --check`: exit 0, after `slot --fetch … rustfmt` on my file.
- `cargo clippy --locked -p spindle-conformance --all-targets -- -D warnings`: exit 0.

**4. Judgement findings:** none.

**5. Attacked and not broken:**
- Nested structs and Optional lists: they match ESS, whether written or not.
- Reads through a reference: `null` in every case (no row named, the field missing, a `null` held, an `omitted_when_absent` field on the far row).
- Ordered views: the order matches. ESS ranks `Null` lowest and the host ranks a missing value lowest.
- Filters with parameters, paging and `total`: match.
- `wire:` renames: both sides key rows by the field name.
- Aggregate views: skipping them is right, because the host already answers every aggregate field, `null` included.
- `shape:` presence: both sides read it from the same IR.
- Served views: the only change in `spindle-host` is a doc comment at `view.rs:1235`, and the served-view case is green at base and head.
- A `state` field not typed as the state: the compiler refuses that model, so nobody reaches it.
- Presence on an inline view field: the loader refuses it. This means the `related_read(&field.name).is_some() ||` branch at `crates/spindle-conformance/src/target.rs:299` can never change the result, since a reference read never carries presence. It does no harm and I am not raising it.
- Mutants: replacing `or_insert` with `insert` is caught by the unit's own `view_rows_agree…` test. Removing the aggregate skip, or the `is_optional` filter, changes nothing anyone could observe.

**6. Paths written outside the worktree** (all under `<local path>`):
- `run1.log` to `run6.log`
- `base.log`
- `suite.log`
- `fmt.log`
- `clippy.log`
- `run.sh`
- `commit-msg.txt`
- `declared.txt`: empty, left behind by a command the hook refused.

The base worktree `<local path>` was created, finished with `--discard-cache` and removed with `worktree gc --apply`.

**7.**
```findings
[]
```

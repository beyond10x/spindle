---
format: aep.planning-md/3
id: review-result:wave31-adversary-U5-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 31 U5
relations:
- reviews: story:spec-change-over-stored-rows
revision: 1
---
```
unit: story:spec-change-over-stored-rows (wave 31 U5), findings cover edc2b026; tests committed as fe83e63c on unit/w31-spec-change
verdict: NEEDS-CHANGE
cases: executed 490→497, red 2
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 9 paths (part 6), plus one managed worktree I created and then finished
needs-coordinator: whether "retyped by structure" (D1) is meant to refuse an enum value that was only added, or only a change no stored row can be read under (finding 2)
```

The unit refuses two specification changes that every stored row can still be read under (CONFIRMED / NEEDS-CHANGE). The unit's own fit tests also let three mutants of `fit.rs` through. Everything else I attacked held.

**1. Diff stat** (`git --no-pager diff --stat edc2b026 HEAD`). Both are test files.
```
 .../tests/spindle-kernel/adversary_w31_u5.rs       | 279 +++++++++++++++++++++
 crates/spindle-kernel/tests/spindle-kernel/main.rs |   2 +
```

**2. Cases added** in `crates/spindle-kernel/tests/spindle-kernel/adversary_w31_u5.rs` (SQLite store, sqlite feature). Each runs in well under 1 s.
- `a_field_made_required_that_every_row_holds_fits`: **RED**. `Optional<String>` becomes `String` and every row holds a value.
- `an_enum_value_added_fits_stored_rows`: **RED**. The enum `[Fiction, Poetry]` becomes `[Fiction, Poetry, Drama]`.
- `a_field_made_optional_fits_stored_rows`: green. It catches mutant M1.
- `a_field_made_required_misfits_where_a_row_lacks_it`: green, expects exactly one `Required`. It catches M3.
- `a_removed_field_held_only_as_null_fits`: green. It catches M2.
- `a_renamed_field_misfits_as_removed_and_required`: green.
- `an_enum_value_removed_misfits_rows_holding_the_field`: green.

Red output from the first run of the module alone (`cargo test --locked -p spindle-kernel --test spindle-kernel adversary_w31_u5`; the first attempt did not compile because the `Store` import was missing, and was fixed before any assertion ran):
```
---- adversary_w31_u5::an_enum_value_added_fits_stored_rows stdout ----
assertion `left == right` failed: every stored row reads under the new specification
  left: Misfits([Retyped { entity: "keep.shelf.Book", field: "genre", rows: 1 }])
 right: Fits { unrecorded: false }
---- adversary_w31_u5::a_field_made_required_that_every_row_holds_fits stdout ----
assertion `left == right` failed: every stored row reads under the new specification
  left: Misfits([Retyped { entity: "keep.shelf.Book", field: "note", rows: 1 }])
 right: Fits { unrecorded: false }
test result: FAILED. 5 passed; 2 failed; 0 ignored; 0 measured; 528 filtered out; finished in 0.02s
```

**Mutants.** I ran them in a separate managed worktree at edc2b026, never in the unit's tree. The filter was `cargo test ... -- fit:: adversary_w31_u5`, which runs the unit's 10 fit cases plus my 7. All 10 of the unit's cases passed under every mutant:
- **M1**: `reads` reduced to `now == was` (`fit.rs:347`). Only my cases failed (3), among them `a_field_made_optional_fits_stored_rows`.
- **M2**: `if !value.is_null()` changed to `if true` (`fit.rs:335`). Only my cases failed (3), among them `a_removed_field_held_only_as_null_fits`.
- **M3**: `is_none_or(optional)` changed to `is_none()` (`fit.rs:150`). Only my cases failed (3), among them `a_field_made_required_misfits_where_a_row_lacks_it`.

**3. Suite**, run after the cases existed: `cargo test --locked -p spindle-kernel --no-fail-fast` exited 101.
```
test result: ok. 15 passed; 0 failed; 0 ignored ...          (lib)
test adversary_w31_u5::a_field_made_required_that_every_row_holds_fits ... FAILED
test adversary_w31_u5::an_enum_value_added_fits_stored_rows ... FAILED
test result: FAILED. 480 passed; 2 failed; 53 ignored; 0 measured; 0 filtered out; finished in 7.93s
```
The "before" count of 490 (15 + 475) comes from the implementor's `gate1-test-spindle-kernel.log`. `cargo fmt -p spindle-kernel --check` exits 0.

**4. Findings** (cover edc2b026)
1. **`crates/spindle-kernel/src/fit.rs:347`**, acceptance, NEEDS-CHANGE, introduced. A field made required that every row holds is refused as `Retyped`. The module's own doc says only "made required, while rows lack it" misfits, and the Outcome says a change that keeps every row readable starts. The case is reachable: a consumer fills in an optional field on every row, then makes it required, and the next `run` exits 3. Suggested fix: `reads` should also accept `was == {"optional": now}`; the `Required` check already handles rows that lack a value. The kernel rule is shared by every store, so this hits PostgreSQL too; I did not run a PostgreSQL case.
2. **`crates/spindle-kernel/src/fit.rs:381`**, acceptance, CONFIRMED, introduced. Adding an enum value refuses every row that holds the field, because an enum is written as its set of values and `reads` compares by equality. D1 ("retyped by structure") reads literally as allowing this. The Outcome ("cannot read a stored row") does not. A struct that gains an optional field has the same shape. This is the open question in `needs-coordinator`.
3. **`crates/spindle-kernel/tests/spindle-kernel/fit.rs`**, mutant, CONFIRMED, introduced. No case of the unit makes a field optional, makes a field required, or holds a field as `null`, so M1, M2 and M3 all survive the unit's suite. My kept green cases close all three.
4. **`crates/spindle-kernel/src/fit.rs:280`**, judgement, note, CONFIRMED, introduced. I read this and did not run it. When a domain moves to another component, the old component's store prints "the entity is removed" for an entity that still exists elsewhere in the specification (`Shapes::owned`). The refusal itself is right; the line is misleading.
5. **`crates/spindle-kernel/src/fit.rs:372`**, judgement, note, CONFIRMED, introduced. I read this and did not run it. The shape comparison ignores a newtype's `alphabet`, `prefix` and `invariants`, and a struct's invariants. A tightened constraint that stored values break is therefore accepted.

**5. Attacked and could not break**
- A removed field, retype through a named type, an added required field, a removed entity, a removed state, a renamed field and a removed enum value all misfit with the right entity, field and row count.
- A refused change leaves the record as it was, in both the kernel and the CLI paths.
- The record does not show in `events_after`, and the feed filter is tested.
- Exit 3 and its stdout text, for a served run, a stdin run and PostgreSQL, come through `misfit_refused` and `HostError::Misfit`.
- In a served run, a misfit in one component writes no record for any component.
- A mixed PostgreSQL and SQLite run cannot leave a record behind: in that case the SQLite store is always in an empty temporary directory.
- The implementor's change to `adversary_state_move` keeps what the test asserts; it only leaves the new record event out of the count.
- The model digest is computed from the model alone (ESS `SuiteProvenance::computed`).

**6. Written outside the worktree**
- `<local path>` holds `case-alone.log`, `commit-msg.txt`, `mutant-m1-optional-clause-dropped.log`, `mutant-m2-nulls-counted.log`, `mutant-m3-made-required-ignored.log`, `mutants.sh`, `suite-kernel.log` and `wait.sh`. That is the directory plus 8 files.
- `<local path>` is a managed worktree I created. Its source is back to unchanged, its build output is discarded, and it is marked finished; it has not been removed.

```findings
- file: crates/spindle-kernel/src/fit.rs
  line: 347
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a field made required that every stored row holds is refused as Retyped, although the module's own rule and the Outcome say it fits
- file: crates/spindle-kernel/src/fit.rs
  line: 381
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: adding an enum value refuses every row holding the field because enums are compared by set equality, though every stored value still reads
- file: crates/spindle-kernel/tests/spindle-kernel/fit.rs
  category: mutant
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the unit's fit cases stay green with reads' optional clause dropped, null values counted as held, or the made-required check removed
- file: crates/spindle-kernel/src/fit.rs
  line: 280
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: an entity whose domain moved to another component is reported as removed, which misnames the refusal
- file: crates/spindle-kernel/src/fit.rs
  line: 372
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the shape comparison ignores newtype alphabet, prefix and invariants and struct invariants, so a tightened constraint that stored values break is accepted
```

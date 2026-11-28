---
format: aep.planning-md/3
id: review-result:wave33-adversary-U4-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 33 U4
relations:
- reviews: story:entity-invariant-over-stored-rows
revision: 1
---
unit: story:entity-invariant-over-stored-rows, findings cover head de943f100 (the unit's 1e90a5d23 plus my test commit)
verdict: CONFIRMED (two test gaps and one note; nothing that should hold the unit as a blocker)
cases: executed 532→537, red 1
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 9 paths (listed in part 6)
needs-coordinator: whether the one red case (a note, INFEASIBLE) should hold the unit, since it leaves the spindle-kernel suite red

The unit's code does what the story asks. I found two mutants its own tests let through, and I added green tests that catch both. The one red case is a statement containing a line break, which prints over two lines. I found nothing that reaches that case.

**1. Diff since the unit's head (test files only)**
```
 .../tests/spindle-kernel/adversary_w33_u4.rs       | 257 +++++++++++++++++++++
 crates/spindle-kernel/tests/spindle-kernel/main.rs |   2 +
 2 files changed, 259 insertions(+)
```
The `mod adversary_w33_u4;` line sits under `#[cfg(feature = "sqlite")]`, like `mod fit;`. Commit `de943f100` was made with `b10x-gates bot`. The check for names and home paths printed nothing.

**2. Cases added** in `<local path>`. Each runs well under 1 s; the module took 0.11 s.
- `two_stored_rows_breaking_one_invariant_misfit_with_two_rows`, green: two of three rows break `title.count <= 10`, giving one misfit with `rows: 2` and a line ending `": 2"`.
- `the_misfit_count_equals_the_rows_at_rest_refuses`, green: a fixed-seed property over 40 drawn rows and three invariants. The misfit's row count equals the number of rows `at_rest` refuses, and no misfit is returned when that number is 0.
- `rows_of_an_entity_the_record_lacks_are_judged`, green: the recorded shapes have no Book, the store holds a Book that breaks the invariant, and one misfit comes back.
- `a_row_at_the_bound_fits`, green: titles of 10 characters and of 0 characters fit, and the new specification is recorded.
- `an_invariant_holding_a_line_break_misfits_on_one_line`, **red**. Output of its first run alone, before the suite ran:
```
assertion `left == right` failed: one line: "keep.shelf.Book invariant `title != 'one\ntwo'`: the invariant does not hold; stored rows breaking it: 1"
  left: 2
 right: 1
```
My first version of this case used a YAML literal block, and ESS refused that model (`[type_mismatch] ... Text literal `10\nor weight <= 2``). That was my construction error, not a finding, so I rewrote the case to put the line break inside a text literal.

**Mutant probes.** These ran in my own managed tree, `w33-u4-adv-mutant`, never in the tree under attack. The `fit::` filter ran the unit's existing fit cases plus my module:
- `*broken += 1` changed to `*broken = 1` (`fit.rs:519`): every `fit::` case passed (24 passed, 3 failed, all 3 mine). The two count cases catch it.
- The invariant block moved after `let Some(was) = was else` (`fit.rs:214`): every `fit::` case passed (25 passed, 2 failed, both mine). `rows_of_an_entity_the_record_lacks_are_judged` catches it.

**3. Suite runs (after the cases existed)**
- `cargo test --locked -p spindle-kernel --no-fail-fast` exited 101: lib `15 passed`; test binary `521 passed; 1 failed; 70 ignored`. The failure is `adversary_w33_u4::an_invariant_holding_a_line_break_misfits_on_one_line`.
- The same command with `-- --skip adversary_w33_u4` (my 5 cases deselected) exited 0: lib `15 passed`; test binary `517 passed; 0 failed; 70 ignored; 5 filtered out`.
- `cargo fmt -p spindle-kernel --check` exited 0, after I wrapped one line in my file.
- `cargo clippy --locked -p spindle-kernel --all-targets -- -D warnings` exited 0.

**4. Findings** (covering `de943f100`)

| file:line | category | severity | verdict | origin | what reaches it |
|---|---|---|---|---|---|
| `crates/spindle-kernel/src/fit.rs:519` | mutant | warning | CONFIRMED | introduced | Any store with two or more rows that break one invariant. The count is wrong under the mutant, and only my two cases catch it. |
| `crates/spindle-kernel/src/fit.rs:214` | mutant | note | CONFIRMED | introduced | Nothing found. It needs an entity's rows in a store whose record lacks that entity, which the base code also treats as a case. |
| `crates/spindle-kernel/src/fit.rs:442` | boundary | note | INFEASIBLE | introduced | Nothing found. It needs a line break inside a text literal of an entity invariant; I built that state. Possible fix: escape the line break in the `Display` arm. |

**5. Attacked and not broken**
- An invariant left `Unknown` is treated exactly as `at_rest` treats it (shown by the property case).
- Invariants are judged on every path that checks a store: the stdin `Shapes::of`, the host and PostgreSQL `Shapes::owned`, and `recorded_empty`. Each passes a model.
- The record format: `Shape` still has only `fields` and `states`.
- The order of misfits follows the order the invariants are declared in.
- The docs, the CLI help, the status row and the CHANGELOG describe the same behaviour as the code.
- The two fixtures differ only by the invariant line and the summary.

I did not run the PostgreSQL lane.

**6. Paths written outside the worktree**
- In `<local path>`:
  - `alone.log`
  - `mutant-count.log`
  - `mutant-order.log`
  - `suite.log`
  - `suite-before.log`
  - `clippy.log`
  - `fit-mutant-b.rs`
  - `commit-msg.txt`
- `<local path>`: my probe tree. It is clean, its build cache is discarded and it is marked finished. I did not run `worktree gc --apply`, because without `--id` it could also remove other agents' finished trees.

**7.**
```findings
- file: crates/spindle-kernel/src/fit.rs
  line: 519
  category: mutant
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the unit's cases never have two rows breaking one invariant, so the mutant `*broken = 1` passes them; adversary_w33_u4's count and property cases catch it
- file: crates/spindle-kernel/src/fit.rs
  line: 214
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: moving the invariant misfits after the `was` check passes every unit case; rows_of_an_entity_the_record_lacks_are_judged catches it
- file: crates/spindle-kernel/src/fit.rs
  line: 442
  category: boundary
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: Misfit::Invariant prints the statement verbatim, so a line break inside a text literal of an invariant splits one misfit over two printed lines, against D2 A's one line per invariant
```

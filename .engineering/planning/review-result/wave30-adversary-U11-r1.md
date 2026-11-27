---
format: aep.planning-md/3
id: review-result:wave30-adversary-U11-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 30 U11
relations:
- reviews: story:affects-over-input-list
revision: 1
---
I couldn't break U11. All 12 attack cases pass on the unit's head, and the kernel's results match ESS 0.99.2's interpreter in every scenario I tried. Four small mutants of `affects.rs` pass the unit's own `affects_each` tests but fail mine, so those gaps are now covered.

```
unit: story:affects-over-input-list, head 5db97713 (adversary cases committed on top as aaae5192)
verdict: REFUTED (no red case; the parity claim held in every case tried)
cases: executed 461→473, red 0
origin: introduced 3 / pre-existing 0 / undecided 1
wrote-outside-worktree: 10 paths (part 6)
needs-coordinator: none
```

**1. Diff stat** (`git --no-pager diff --stat 5db97713..HEAD`): only test files changed.
```
 crates/spindle-kernel/tests/fixtures/adversary-w30-u11/domains/book.yaml | 444 +++
 crates/spindle-kernel/tests/fixtures/adversary-w30-u11/system.yaml       |  38 +
 crates/spindle-kernel/tests/spindle-kernel/adversary_w30_u11.rs          | 773 +++
 crates/spindle-kernel/tests/spindle-kernel/main.rs                       |   1 +
```

**2. Cases added**, in `crates/spindle-kernel/tests/spindle-kernel/adversary_w30_u11.rs`. Where a case says "matches ESS", the kernel's answers and rows were compared with the interpreter's on both the memory and the SQLite store.
- `an_optional_member_left_out_clears_the_held_field_as_the_interpreter_does`: an element that leaves out an Optional member, or sets it to `null`, clears that field on the held row. Matches ESS.
- `creates_and_moves_over_one_list_agree_with_the_interpreter`: two `each:` entries over one list, create-or-update then move, and also move then write. A row created earlier in the same outcome gets moved. Matches ESS.
- `an_element_filter_left_unknown_skips_the_element_as_the_interpreter_does`: a filter on an absent Optional member comes out Unknown and skips the element. Matches ESS.
- `each_row_an_element_writes_mints_its_own_token`: `{generated: true, present: true}` gives every row a different token, on both stores.
- `one_element_breaking_an_invariant_writes_nothing_of_the_group`: when one element breaks an invariant, both ESS and the kernel refuse the command. Nothing is written: not the subject, not the row an earlier element updated, not a row it created. Checked on memory and SQLite.
- `decimal_identities_spelled_otherwise_name_the_held_cells`: `1.0`, `2.50` and `3e0` name the cells held as `1`, `2.5` and `3`; `4.0` creates a cell. Matches ESS.
- `a_negative_zero_identity_names_the_cell_held_as_zero`: `-0.0` names the cell held as `0`. Matches ESS.
- `an_integer_identity_written_with_a_fraction_agrees_with_the_interpreter`: an `Integer` identity written `1.0` names the counter held as `1`. Matches ESS.
- `one_cell_named_in_two_spellings_is_not_distinct`: a list with `1` and `1.0` is refused as `duplicated`. Matches ESS.
- `an_element_naming_the_subject_in_another_spelling_is_skipped`: subject `1` with element `1.0`, and subject `2.0` with element `2`; both elements are skipped. Matches ESS.
- `a_row_created_by_another_writer_after_the_scan_refuses_the_whole_group`: a test store lets another writer create the row an element would create, between the scan and the commit. The command fails with a store conflict, nothing of the group lands, and the other writer's row stands.
- `a_row_no_element_names_created_after_the_scan_refuses_the_whole_group`: the same, with another writer creating a row of that entity that no element names. The scan condition alone refuses the group.

Red output when written: none of these went red. The first run (`cargo test --locked -p spindle-kernel --test spindle-kernel adversary_w30_u11`) failed one case: `memory: n1 holds a token`. That was my fixture's mistake: an Optional generated field stays absent unless `present: true`. After fixing the fixture, 12 passed. Each case runs well under 1 s; all 12 together take 0.06 s.

Mutants, run in a separate managed worktree; the attacked tree was not touched. Command: `cargo test -p spindle-kernel --test spindle-kernel -- affects_each adversary_w30_u11`.

| Mutant in `crates/spindle-kernel/src/execute/affects.rs` | Unit's `affects_each` | Adversary module |
|---|---|---|
| m1 :350 drop `after.fields.remove(&read.target)` | green | 2 red |
| m2 :294 `== Truth::True` → `!= Truth::False` | green | 1 red |
| m3 :303 ignore rows written earlier in the outcome (read only the stored row) | green | 2 red |
| m4 :285 never skip the subject | 2 red | 1 red |
| m5 :379 drop `generator.rewind(base)` | green | 1 red |

**3. Suite run** (after the cases existed):
- `cargo test --locked -p spindle-kernel --no-fail-fast`: `test result: ok. 15 passed` and `test result: ok. 458 passed; 0 failed; 50 ignored`. Exit 0, 31 s wall.
- With my module skipped (`-- --skip adversary_w30_u11`): `15 passed` and `446 passed; 0 failed; 50 ignored; 12 filtered out`. Exit 0, 14 s.
- `cargo clippy --locked -p spindle-kernel --all-targets -- -D warnings`: exit 0, `Finished`.
- `cargo fmt -p spindle-kernel --check`: exit 0. I formatted my own file with `rustfmt --edition 2024`.
- The cli crate was not run; I added nothing there.

**4. Judgement findings.** These are the mutants from part 2, against tree 5db97713. Each one is reachable by any consumer's `each:` entry with that feature: an Optional member, a `where:`, two entries over one list, or a generated `sets:` value. The unit's tests did not cover them; my kept cases now do.
- m1 at :350: no unit case updates a held row with an Optional member left out. CONFIRMED, introduced.
- m2 at :294: no unit case has a filter that comes out Unknown. CONFIRMED, introduced.
- m3 at :303: no unit case has two entries acting on the same row, so a later entry reading stale rows would pass. CONFIRMED, introduced.
- m5 at :379: no fixture with an `affects:` entry uses a generated value, at the base or now (I checked with grep but did not run the mutant at the base). CONFIRMED, undecided.

**5. Attacked and could not break:**
- an empty list, duplicates under `distinct:`, and an element naming a row that does not exist (unit cases plus mine)
- elements whose Optional members are absent or `null`
- Integer and Decimal identities in other spellings, including `-0.0` and the subject itself
- one list that both creates and moves, in either order
- a filter that comes out Unknown
- the `distinct:` refusal beside the entry
- atomicity when an invariant breaks, and when another writer creates a row after the scan, on memory and SQLite
- stale mentions of the old refusal in docs or code (none found)

**6. Paths written outside the worktree:**
- `<local path>`:
  - `run1.log`
  - `run2.log`
  - `clippy.log`
  - `suite-after.log`
  - `suite-before.log`
  - `mutants.sh`
  - `mutants/` (five `.diff` and five `.log` files, plus `summary.txt`)
  - `commit-msg.txt`
- `<local path>`: the managed mutant worktree. I cleaned it and marked it finished with `worktree finish --discard-cache`; it is not removed.

**7. Findings block**
```findings
- file: crates/spindle-kernel/src/execute/affects.rs
  line: 350
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: dropping the removal of an Optional field an element leaves out passes every affects_each case; adversary_w30_u11 now catches it
- file: crates/spindle-kernel/src/execute/affects.rs
  line: 294
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: letting an Unknown element filter select the element passes every affects_each case; adversary_w30_u11 now catches it
- file: crates/spindle-kernel/src/execute/affects.rs
  line: 303
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: reading the stored row instead of the row an earlier entry wrote passes every affects_each case; adversary_w30_u11 now catches it
- file: crates/spindle-kernel/src/execute/affects.rs
  line: 379
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: undecided
  message: dropping the per-row generator rewind passes every affects_each case because no affects fixture mints a value; adversary_w30_u11 now catches it
```

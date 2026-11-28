---
format: aep.planning-md/3
id: review-result:wave32-adversary-U4-r3
kind: review-result
status: active
title: Adversary, round 3, on wave 32 U4
relations:
- reviews: story:seed-under-a-lease
revision: 1
---
unit: story:seed-under-a-lease round 3, head e340d6b7c (fix 84316b4bb + one adversary commit, tests only)
verdict: REFUTED
cases: executed 172→174, red 0
origin: introduced 0 / pre-existing 0 / undecided 0
wrote-outside-worktree: 5 new paths (part 6)
needs-coordinator: none

I found no break in `gather_seed`. My files are unchanged between 6a08a3e2a and 84316b4bb (the diff is empty). All 9 of my cases now pass, including round 2's red case `a_release_rolled_back_and_forward_with_the_same_seed_skips_it`.

The counts are the parallel part of `task check:postgres`. The 172 is the 174 run minus my 2 new cases; I did not run the lane without them.

## 1. Diff (84316b4bb..HEAD)
```
 .../postgres/replicas/adversary_w32_u4.rs          | 64 ++++++++++++++++++++++
 .../tests/spindle-kernel/adversary_w32_u4.rs       | 49 +++++++++++++++++
```
Both are my own test modules; no implementation file changed.

## 2. Cases added (both green on the first run, kept as property tests)
- **`spindle-kernel adversary_w32_u4::postgres::a_gathering_is_refused_once_the_middle_lease_was_taken`** (1.3 s): three tables hold the same record, and another process takes the lease of the middle one. The gathering is refused and every row is unchanged. Your kernel test only takes the last table's lease.
- **`spindle-cli postgres::replicas::adversary_w32_u4::records_in_three_tables_two_alike_and_one_different_are_refused`** (1.9 s): `audit` and `board` hold equal records and `ledger` holds one that differs only in `done`. The run exits 1 before serving, names all three tables, sends no entry, and leaves every row as it was. The two equal records do not outvote the third.

## 3. Suite run (after the cases existed)
- `slot --timeout 30m --tokens 4 sh -c "<adv-scratch/lane.sh>"`, which runs `task check:postgres`:
  - Parallel part: `Summary [34.094s] 174 tests run: 174 passed, 1510 skipped` (`within: 34.578 s of 100 s`).
  - Timing case run alone: `1 test run: 1 passed` (`within: 10.227 s of 20 s`).
  - `LANE: exit 0 in 47 s`.
- Clippy on both crates' test targets (`-D warnings`): exit 0. `cargo fmt --check`: exit 0.

## 4. Findings
None.

## 5. Attacked and not broken
- **A takeover of any one of the leases involved:** the statement checks each table's lease row for this process's token and session and locks it. The write needs every one of those checks to pass, and so does each delete. Covered by the middle-lease case above, your last-table case, and my round-2 cases for the destination lease and a takeover in flight.
- **Three tables, two alike and one different:** refused. `read` refuses as soon as any record differs from the first one found.
- **Is the body compare complete?** Yes. It compares the whole JSON value: digest, done and rejected. The digest is part of the body, so a record "equal in body but written under another digest's file" cannot exist. `rejected` is written from a `BTreeSet`, so its order is fixed.
  - One mismatch: a row without the `rejected` field would compare unequal to one with `[]`, although both read as the same record. Nothing reaches it, because every writer writes the field.
- **Kill mid-gather:** it is one statement, so all of it lands or none does.

## 6. Written outside the worktree (new this round)
- `<local path>`
- `…/adv-scratch/r3-lane.out`
- `…/adv-scratch/r3-clippy.out`
- `…/adv-scratch/r3-commit-msg.txt`
- the round-1 scripts reused unchanged: `…/adv-scratch/mine.sh` and `lane.sh`

That is 4 new files plus the reused scripts. I took no worktree session lease, so there is none to release.

## 7.
```findings
[]
```

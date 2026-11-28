---
format: aep.planning-md/3
id: review-result:wave32-adversary-U4-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 32 U4
relations:
- reviews: story:seed-under-a-lease
revision: 1
---
unit: story:seed-under-a-lease round 2, head 6a08a3e2a (fix 2c2a00518 + one adversary commit, tests only)
verdict: CONFIRMED
cases: executed 167→170, red 1
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 5 new paths (part 6)
needs-coordinator: none

The fix closes round 1's F1. My round-1 files are unchanged between 054166001 and 2c2a00518 (the diff is empty), and all 4 of my round-1 cases now pass. The move itself held under every race I could build. One new defect: a rollback followed by a roll-forward leaves the same record in two tables, and every start then exits 1.

Case counts are the parallel part of `task check:postgres`. 167 is the 170 run minus my 3 new cases; I did not run the tree without them.

## 1. Diff (2c2a00518..HEAD)
```
 .../postgres/replicas/adversary_w32_u4.rs          |  54 +++++++++++
 .../tests/spindle-kernel/adversary_w32_u4.rs       | 107 +++++++++++++++++++++
```
Both are my own test modules; no implementation file changed.

## 2. Cases added
- **`postgres::replicas::adversary_w32_u4::a_release_rolled_back_and_forward_with_the_same_seed_skips_it`**: RED, 2.1 s alone (4.1 s in the lane).
  - Steps, all with the same `seed.json`:
    1. Release 1 seeds it.
    2. Release 2 moves the record into `audit`'s table.
    3. Rolled back to release 1, the run reads only `board`. Finding no record there, it applies D5 B (the case you parked) and writes an equal complete record into `board`.
    4. Rolled forward to release 2, the run should skip the seed.
  - Got:
    ```
    rolled forward with the same file, both records {"digest":"sha256:c9ae1b75…","done":1,"rejected":[]}, the run exited exit status: 1 instead of skipping the seed: b10x-spindle: reading the seed record in spindle_test_fiptwrgztgik.spindle_audit_p_lease: the seed is recorded in more than one lease table, spindle_test_fiptwrgztgik.spindle_audit_p_lease and spindle_test_fiptwrgztgik.spindle_board_p_lease: keep the row `seed` of one of them and remove the others, so a start knows which entries are done
    ```
- **`spindle-kernel adversary_w32_u4::postgres::a_move_is_refused_once_the_lease_it_moves_to_was_taken`**: GREEN, 1.3 s, kept. Another process takes the lease of the destination table; the move is refused and neither row changes. Your kernel test only takes the source lease, so without this case a mutant that drops the destination's token check stays green.
- **`…::a_move_in_flight_during_a_takeover_of_its_source_lands_nothing`**: GREEN, 0.5 s, kept. The move blocks behind a take that holds the source lease row, then lands nothing in either table once the take commits a later token.

## 3. Suite run (after the cases existed)
- `slot --timeout 30m --tokens 4 sh -c "<adv-scratch/lane.sh>"`, which runs `task check:postgres` with `NEXTEST_NO_FAIL_FAST=1`:
  - `Summary [33.946s] 170 tests run: 169 passed, 1 failed, 1510 skipped`. The one failure is the red case above.
  - `within: 34.446 s of 100 s, the command failed with status 100`; `LANE: exit 201 in 37 s`.
- Clippy on both crates' test targets (`-D warnings`): exit 0. `cargo fmt --check`: exit 0.

## 4. Findings
**R2-F1 (NEEDS-CHANGE, introduced): two equal records stop every start.**
- **Where:** `crates/spindle-cli/src/database.rs:509` (`if found.len() > 1`) refuses any two records, even when they are equal and name the file given.
- **Effect:** every replica restarts into exit 1 until someone deletes a row by hand.
- **What reaches it:** an ordinary rollback and roll-forward. A rolling deploy reaches it too: a release-1 standby leads after release 2 moved the record, and release 2 leads again later.
- **Fix to name (not applied):** when every record found holds the same body, use it and let the next write remove the others; refuse only when they differ.
- **Related, already parked:** the rollback step itself is the case you parked. If release 2 had left the seed part done, the rolled-back release would mark it complete.

## 5. Attacked and not broken
- **Takeover of either lease between the read and the move:** refused, both rows unchanged. Covered by the two kernel cases above and your source-lease case.
- **Kill mid-move:** the move is one statement, so it lands whole or not at all; two rows or none cannot result. Reasoned, not run with a kill. Forget, by contrast, runs one statement per table, but its gap sits inside the reset-to-forget window that existed before round 2.
- **Component renamed (one added, one removed in a release):** a rename of a component that does not hold the record moves the record correctly. A rename of the one that holds it is the case you parked. Reasoned from the code, not run.
- **Two standbys starting at once on a store that needs a move:** leases are taken in name order and a process stops at the first one held elsewhere, so one process leads and only it reads and moves. Reasoned, not run.

## 6. Written outside the worktree (new this round)
- `<local path>`
- `…/adv-scratch/r2-lane.out`
- `…/adv-scratch/r2-clippy.out`
- `…/adv-scratch/r2-commit-msg.txt`
- the round-1 scripts reused unchanged: `…/adv-scratch/mine.sh` and `lane.sh`

That is 4 new files plus the reused scripts. I took no worktree session lease, so there is none to release.

## 7.
```findings
- file: crates/spindle-cli/src/database.rs
  line: 509
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: two equal seed records left in two lease tables by a rollback and roll-forward of the release that adds a component make every start exit 1 instead of skipping the seed, until a row is removed by hand
```

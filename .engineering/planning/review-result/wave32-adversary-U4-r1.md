---
format: aep.planning-md/3
id: review-result:wave32-adversary-U4-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 32 U4
relations:
- reviews: story:seed-under-a-lease
revision: 1
---
unit: story:seed-under-a-lease, head 054166001 (unit head 32b58f38a + one adversary commit, tests only)
verdict: CONFIRMED
cases: executed 162→165, red 2
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 7 paths (part 6)
needs-coordinator: none

The case count: 162 comes from the implementor's own lane run (`scratch/lane-after.out`: 161 in the parallel part plus 1 timing case). Mine was 164 in the parallel part, and the timing case did not run because that step failed.

## 1. Diff (32b58f38a..HEAD)
```
 .../adversary-w32-u4-grown/domains/audit.yaml      |  30 ++++
 .../adversary-w32-u4-grown/domains/board.yaml      |  97 ++++++++++++
 .../adversary-w32-u4-grown/domains/ledger.yaml     |  30 ++++
 .../fixtures/adversary-w32-u4-grown/system.yaml    |  57 +++++++
 .../tests/spindle-cli/postgres/replicas.rs         |   2 +
 .../postgres/replicas/adversary_w32_u4.rs          | 167 +++++++++++++++++++++
 .../tests/spindle-kernel/adversary_w32_u4.rs       | 121 +++++++++++++++
 crates/spindle-kernel/tests/spindle-kernel/main.rs |   1 +
```
Every path is a test file, a fixture or a `mod` line. No implementation file changed.

## 2. Cases added
The fixture `adversary-w32-u4-grown` is the `ingress` spec with one added component, `audit`, whose name sorts before `board`.

- **`postgres::replicas::adversary_w32_u4::another_seed_file_after_a_component_sorting_first_was_added_is_refused`**: RED, 0.9 s.
  - Setup: release 1 seeds `first.json`. Release 2 starts with `other.json` on a store that holds rows and a complete record.
  - Expected (D7 A): exit 1, naming both digests.
  - Got: the run serves.
    ```
    the run with another seed file on a store holding rows and a record of sha256:c9ae1b75… listened on 127.0.0.1:34871 instead of exiting 1 before serving (D7 A); audit's lease table now holds Some(Object {"digest": String("sha256:0377bcc0…"), "done": Number(2), "rejected": Array []})
    ```
- **`…::a_seed_left_part_done_resumes_after_a_component_sorting_first_was_added`**: RED, 0.9 s.
  - Setup: the record is set to the 3-entry file at `done` 1, the state a leader killed after entry 1 leaves. I built this state through SQL. The next run is release 2 with the same file.
  - Expected: `ended: 2 sent, 2 applied`.
  - Got: the seed is skipped.
    ```
    …the PostgreSQL store holds 1 rows and the row `seed` of spindle_test_mlvntydlxwew.spindle_audit_p_lease records no seed, … the seed is taken as applied there, so no entry is sent
    ```
- **`…::a_seed_left_part_done_resumes_on_the_same_components`**: GREEN, 0.8 s. This is the control: the same built record with the unchanged spec resumes and sends entries 2 and 3. So the record I built is one the run honours, and the added component alone causes the skip.
- **`spindle-kernel adversary_w32_u4::postgres::a_record_write_in_flight_during_a_takeover_lands_nothing`**: GREEN, 0.4 s, kept as a property test. A record write blocks behind another session that holds the lease row while moving its token on. Once that session commits, the write is refused with "no longer names token 1" and the row is unchanged. This confirms claim 1 under a real race.

No case added takes over 5 s.

## 3. Suite run (after the cases existed)
- Command: `slot --timeout 30m --tokens 4 sh -c "<adv-scratch/lane.sh>"`, which runs `task check:postgres` with `NEXTEST_NO_FAIL_FAST=1`.
- Result: `Summary [27.993s] 164 tests run: 162 passed, 2 failed, 1510 skipped`. The 2 failures are the two red cases above.
- `within: 28.457 s of 100 s, the command failed with status 100`; `LANE: exit 201 in 40 s`. The step stays well inside its budget.
- `cargo clippy --locked -p spindle-cli -p spindle-kernel --all-targets -- -D warnings`: exit 0. `cargo fmt --check`: exit 0.

## 4. Findings
**F1 (NEEDS-CHANGE, introduced): the seed record moves when a component sorting first is added.**
- **Where:** `crates/spindle-cli/src/database.rs:403`. The record's table is taken from `self.components.first()`, so it depends on which components the current spec has. Release 2's `audit` table has no record while the store still holds rows, and `seed.rs:370` (D5 B) then treats the seed as applied.
- **Effect:** another seed file is served silently instead of exiting 1 (D7 A, the concepts page, the `--seed` help). A seed left part done is never resumed (the Outcome's "resumes after the count"). In both cases a complete record is written to the new table, which hides the loss.
- **What reaches it:** a later release of a consumer's spec that adds a component whose name sorts first, run with `store migrate` on the existing PostgreSQL store. The Outcome itself names "a changed seed file in a later release" as a case to handle.
- **Fix to name (not applied):** before falling back to D5 B, read the `seed` row from every component's lease table (or keep it in every one, D4 C), so that adding or renaming a component cannot hide a record.

## 5. Attacked and not broken
- **Takeover while the old leader's record write is in flight:** refused. Kernel property above.
- **Leader killed between an entry's commit and its record write:** a restart resends that entry, the same window as `seed.json` on `--state`. Not introduced, and not deterministically testable.
- **Several components:** the record goes to the first component's table and `Host::rows` counts all of them. That is consistent, apart from F1.
- **`--state` path:** `Record::State` is reached only without a lease, and `--state` on PostgreSQL exits before this point. Unchanged.
- **Reset racing the seed:** `reset_seeded` cuts, writes `done` 0, then sends again. The only gap is between the store being emptied and `done` 0 being written, which `--state` shares.
- **D5 B on a 0.26.0 store:** behaves as decided.
- **Contract:** the line wording, exit 1 and the record's JSON shape match the story and docs, except where F1 applies.
- **Teardown order and stopping the lease thread:** an explicit `Ask::Stop` ends the thread even while `SeedRow` copies are still alive. Unchanged.

## 6. Written outside the worktree
- `<local path>`
- `…/adv-scratch/mine.out`
- `…/adv-scratch/mine2.out`
- `…/adv-scratch/mine3.out`
- `…/adv-scratch/lane.sh`
- `…/adv-scratch/lane.out`
- `…/adv-scratch/clippy.out`
- `…/adv-scratch/commit-msg.txt` (an eighth file, also in adv-scratch)

I did not take a worktree session lease of my own, so there is none to release.

## 7.
```findings
- file: crates/spindle-cli/src/database.rs
  line: 403
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the seed record lives in the lease table of whichever component sorts first, so a release that adds a component sorting first finds no record on a store with rows and D5 B silently skips both another seed file (D7 A says exit 1) and a part-done seed (should resume), writing a complete record over the loss
```

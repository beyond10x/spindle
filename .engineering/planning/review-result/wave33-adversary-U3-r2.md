---
format: aep.planning-md/3
id: review-result:wave33-adversary-U3-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 33 U3
relations:
- reviews: story:seed-record-after-a-component-is-removed
revision: 1
---
unit: story:seed-record-after-a-component-is-removed (U3, round 2), head 3efa8cd97 (fix 185b156f1 plus my two test commits)
verdict: CONFIRMED (one red case; its finding is INFEASIBLE because the test builds the failure itself)
cases: executed 187→189, red 1
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 10 paths (part 6)
needs-coordinator: whether to fix or accept the red case, which keeps `task check:postgres` red. While that step fails, the lane's 20 s timing step does not run.

**1. Diff stat** (`git diff --stat 185b156f1..HEAD`; nothing uncommitted)
```
 .../postgres/replicas/adversary_w33_u3.rs          | 268 +++++++++++++++++++--
 1 file changed, 242 insertions(+), 26 deletions(-)
```
Only a test file changed. Commits: c22ff3713 (the cases) and 3efa8cd97 (measured times in the doc comments).

**2. Cases** in `<local path>`

- **Round-1 case, updated and renamed** to `a_reset_while_an_old_release_holds_the_removed_components_lease_is_refused_until_it_ends_then_the_seed_is_sent_again` (:137). **Green**, 1.9 s. It checks:
  - While the old release holds audit's lease, the reset answers 409, the refusal names audit's table and the holder, the tasks stay s1 and s2, audit's record is unchanged and board has no record.
  - After the old process releases the lease, a reset with a malformed world is refused (not 200, not 409), and audit's lease is free again before that answer.
  - A plain reset then answers 200, audit's lease is free while the run is still running, and both records are gone.
  - The next seeded start prints `ended: 2 sent, 2 applied` and has tasks s1 and s2.
- **`a_removal_of_the_old_record_that_fails_after_the_reset_leaves_the_seed_to_be_sent_again`** (:213). **Red**, 1.7 s. A trigger on audit's lease table refuses that one delete, which stands in for a lost claim or a failed statement. The trigger is dropped before the next seeded start. The case asserts that either the reset changed nothing or the next seeded start sends the whole seed. Red output from the run of this case alone (verbatim excerpt):
  ```
  b10x-spindle: the row `seed` of spindle_test_dtvsbduviwuc.spindle_board_p_lease could not be removed after the reset, so a start with that seed may skip the entries it records done: store: PostgreSQL refused a statement on the lease of spindle_test_dtvsbduviwuc.spindle_audit_p_lease: the removal of the seed record failed (SQLSTATE P0001)
  ... already ran on the PostgreSQL store, as the row `seed` of spindle_test_dtvsbduviwuc.spindle_board_p_lease records: no entry is sent
  panicked at .../adversary_w33_u3.rs:244:5:
  the reset answered 200 {"emptied":["board","ledger"]} and left []; audit's table holds None; the next seeded start left [].
  ```
  This answers the question in the brief: when the old record cannot be removed after the reset, the reset is **not** refused. It answers 200 with the stores emptied, the next seeded start reads audit's record (which it then moves into board's table) and sends nothing.
- **`a_reset_that_outlasts_the_lease_keeps_the_removed_components_lease_and_removes_its_record`** (:350). **Green**, 5.0 s on the lane. A run with a 1 s lease resets while loading a world of 1,500 tasks, which takes longer than 1.5 s. An old-release standby asks for audit's lease the whole time. The case asserts:
  - the reset answers 200;
  - the standby never gets the lease;
  - the record is removed.

  Nothing else tests `renew_orphan` or the loop that renews claimed leases. I removed that loop in a copy (patch in scratch, run in a managed worktree) and the case turned red. The reset took 6.5 s, the standby got token 3, the removal was refused with "fenced: the lease of …audit_p_lease no longer names token 2", and the record stayed. So the guard is needed and this case catches its removal.

**3. Lane run** (after the cases existed): `slot --timeout 30m --tokens 4 task check:postgres`, exit 201.
```
Summary [  57.108s] 189 tests run: 188 passed, 1 failed, 1510 skipped
FAIL [   1.676s] (181/189) ... adversary_w33_u3::a_removal_of_the_old_record_that_fails_after_the_reset_leaves_the_seed_to_be_sent_again
within: 57.568 s of 100 s, the command failed with status 100
```
The before count (187) is worked out from this run minus my two added cases; the round-1 case was replaced, not added. None of my cases is among the ten slowest (the slowest test took 13.7 s).
Other commands: `cargo fmt -p spindle-cli --check` passed; `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings` exit 0.

**4. Findings**
- **Removal failing after the reset** — `crates/spindle-cli/src/database.rs:515` (`forget_claimed` is called after the reset has already emptied the stores) with `serve.rs:784`. INFEASIBLE, introduced.
  - **What was measured:** the red case above.
  - **What reaches it:** I found no path that loses the claim while the run keeps its own leases. The claim is renewed in the same loop as the run's own leases. If the session ends, the run's own leases are lost too, because a renewal must come from the same session. So the state is built by the test.
  - **Fix options** (I did not apply either):
    - Remove the old records under the claim before `console::reset` empties anything, and refuse with 500 if that fails. If the reset then fails, the store has rows and no record, so the existing "rows without a record" rule (D5 B) correctly sends nothing.
    - Or, when the removal fails, write a record with done 0 into the first current component's table; a record in a current table wins over old ones.
- **Warning names the wrong table** — `crates/spindle-cli/src/seed.rs:288` (`forget_row`). CONFIRMED, introduced, note. When the old table's removal fails, the line says board's row "could not be removed", but board's row was removed and audit's is the one left. Both the red case and the mutant run show this.

**5. Attacked and not broken**
- **Lease taken by another process mid-claim:** an old process holding the lease at the claim gets 409. After the claim, the old process cannot take the lease: the row is renewed and its session is alive.
- **Leases left held:** none after a 409, after a reset refused for its body, or after a 200. All three were checked while the run was still running.
- **Crash while holding the claim:** the lease is free as soon as its session ends (take checks the holder's session). Shown by reading the take statement, not by a test.
- **Reset with a seed:** it never runs the claim, and it always rewrites board's record, which wins over audit's. No attack found.
- **Lane time:** 57.6 s of the 100 s bound, my cases 1.7 to 5.0 s.

**6. Paths written outside the worktree**
- In `<local path>`: `cases.log`, `renew.log`, `clippy.log`, `lane.log`, `mutant.log`, `mutant.patch`, `commit.txt`, `commit2.txt`, `renew-case.rs`.
- Managed worktree `<local path>`: restored to clean and finished. `worktree gc` keeps it until c22ff3713 is on a remote, so it is yours to remove.
- The lane clusters under the unit worktree's `target/` were removed by the lane script.

**7.**
```findings
- file: crates/spindle-cli/src/database.rs
  line: 515
  category: concurrency
  severity: warning
  verdict: INFEASIBLE
  origin: introduced
  message: a removal of a claimed old table's record that fails after console::reset emptied the stores leaves the reset answering 200 with that record kept, so the next seeded start sends nothing to the emptied store; the failure is built by a trigger, and nothing found loses the claim while the run keeps its own leases
- file: crates/spindle-cli/src/seed.rs
  line: 288
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the warning after a failed removal names the first component's row as not removed when that row was removed and the removed component's row is the one left
```

---
format: aep.planning-md/3
id: review-result:wave33-adversary-U3-r6
kind: review-result
status: active
title: Adversary, round 6, on wave 33 U3
relations:
- reviews: story:seed-record-after-a-component-is-removed
revision: 1
---
The closing path held: none of my attacks turned into a failing case on the unit's head. I found one gap in the suite: no existing test checks the closing path's lease renewal. The case I added now covers it.

```
unit: story:seed-record-after-a-component-is-removed, head 3737f5f67 (findings cover it; my test commit is 602e877a4 on unit/w33-orphan-seed)
verdict: CONFIRMED (one mutant the suite missed, now covered by a kept case that passes; no red case on the head)
cases: executed 201→202, red 0
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 9 paths (part 6)
needs-coordinator: removing the probe worktree spindle-w33-u3-adv6-mutant; worktree gc keeps it until 3737f5f67 is on a remote
```

The 201 is not a separate run: it is this lane run's 202 minus my one case. Round 5's lane run had 200.

**1. Diff since the head** (`git --no-pager diff --stat 3737f5f67..HEAD`)
```
 .../postgres/replicas/adversary_w33_u3.rs          | 95 ++++++++++++++++++++++
 1 file changed, 95 insertions(+)
```
Test file only. Commit 602e877a4 went through `b10x-gates bot -- commit`, and the privacy grep printed nothing.

**2. The case added**

Test name: `after_a_lost_lease_a_standby_of_the_old_release_takes_the_removed_components_lease_only_with_its_record_back`, in `crates/spindle-cli/tests/spindle-cli/postgres/replicas/adversary_w33_u3.rs`, with the helper `standing_by_on_audit`.

- **Setup:** same as round 5's second case (release 2 on a 1 s lease, `board`'s tables locked for 2.5 s, `board`'s token moved on after the claim). In addition, a standby of release 1 asks for `audit`'s lease every 50 ms.
- **What it asserts:** the standby gets `audit`'s lease only once the record is back, and the record ends at `done` 1 of 3.
- **Why round 5's case misses this:** the put-back checks only the lease row's token and session, not its expiry. A claimed lease that runs out unrenewed still gets its record back as long as nobody takes it. So round 5's case passes with or without the renewal.

Results:
- **Head, alone:** passes in 3.7 s.
- **Mutant:** a managed worktree at 3737f5f67 with the `renew_orphan` loop removed from `Leases::closing` (`database.rs:898-900`). Round 5's case still passes there in 3.7 s; mine fails in 3.9 s:
```
panicked at crates/spindle-cli/tests/spindle-cli/postgres/replicas/adversary_w33_u3.rs:1197:9:
assertion `left == right` failed: a standby of release 1 took audit's lease (token 3) while the record removed for the reset of the run that lost its own lease was not back; the reset answered 500 (exit ExitStatus(unix_wait_status(256))): {"refused":"resetting board: reading tasks.board.Task: store: event store deadline exceeded during database statement or lock; ... the row `seed` of spindle_test_jyacvhuvzbjk.spindle_audit_p_lease was not put back: store: fenced: the lease of spindle_test_jyacvhuvzbjk.spindle_audit_p_lease no longer names token 2 and this session, ..."}
      left: None
     right: Some(Object {"digest": String("sha256:257f…"), "done": Number(1), "rejected": Array []})
```

**3. Lane run** (`slot --timeout 30m --tokens 4 task check:postgres`, after the case existed)
```
        PASS [   5.207s] (200/202) ... adversary_w33_u3::after_a_lost_lease_a_standby_of_the_old_release_takes_the_removed_components_lease_only_with_its_record_back
     Summary [  76.352s] 202 tests run: 202 passed, 1510 skipped
within: 77.130 s of 100 s: ...
exit 0
```
The lane's parallel part went from 73.1 s to 77.1 s. My case takes 5.2 s in the lane, so it is over the 5 s reporting mark.

**4. Finding**

| file:line | verdict | origin | what was measured | what reaches it |
|---|---|---|---|---|
| `crates/spindle-cli/src/database.rs:898` | CONFIRMED | introduced (round 5's fix) | With the `renew_orphan` loop in `Leases::closing` removed, round 5's case and every other lane case still pass; my case fails at `adversary_w33_u3.rs:1197` | A rollout or rollback: release 1's standbys ask for `audit` first in name order, and release 2's leader loses its lease during a reset that has removed the record. Nothing needs to change in the code; the case now guards it |

**5. Attacked and could not break**
- **SIGTERM during a reset after a lost lease:** the reset runs on the main thread, so `Kept`'s `Stop` is always queued after the reset's `Unclaim`. The closing path ends on `Unclaim` and does not delay the exit.
- **The stop bound when the reset never sends `Unclaim`:** no path in `serve.rs` takes a claim without sending `Unclaim` afterwards. When the claim fails or times out, `SeedRow::claim` sends `Unclaim(true)` itself. Not reachable.
- **A reset with a seed during closing:** `reset_seeded` takes no claim, and the closing path with nothing claimed returns at once.
- **A claim arriving during closing:** the run marks the lease lost before closing starts, and the main thread checks that before each exchange. The only gap is microseconds wide, between that check and the claim.
- **A claimed lease kept renewed while another release's leader needs it:** release 1 asks for `audit` first, so it is blocked by the claim before the loss and after it alike, and only until the closing deadline.
- **Docs and CHANGELOG against the code:** `until = 2 × store_wait + statement_wait × (components + 2)` matches "two store operations and the run's wait". `store_wait` uses the same default pool timeouts as `PostgresStore::open`.

**6. Paths written outside the worktree**
- In `<local path>`: `one.sh`, `two.sh`, `case1.log`, `mutant.log`, `mutant.patch`, `lane.log`, `commit.txt`, `finish.log`.
- `<local path>`, the probe worktree I created. I reverted my edits, finished it and released my lease. It is not removed because `worktree gc` keeps it with `no-remote-recovery-proof` for 3737f5f67.

**7. Findings**
```findings
- file: crates/spindle-cli/src/database.rs
  line: 898
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: no case failed with the claimed-lease renewal removed from Leases::closing, because the put-back is fenced by token and session only; adversary_w33_u3 now has a standby case that fails on that mutant
```

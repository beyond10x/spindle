---
format: aep.planning-md/3
id: review-result:wave33-adversary-U3-r3
kind: review-result
status: active
title: Adversary, round 3, on wave 33 U3
relations:
- reviews: story:seed-record-after-a-component-is-removed
revision: 1
---
unit: story:seed-record-after-a-component-is-removed (wave 33 U3); findings cover head 54c1c0c73 (the unit's 23c816a73 plus this round's test commit)
verdict: CONFIRMED
cases: executed 190→194, red 2
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 11 paths (part 6)
needs-coordinator: none

**Summary.** The renew case that kept failing was a race in the test, not a bug in the code. I rewrote it: it is green on head in 5 of 5 runs and red in 3 of 3 runs with the renew loop removed. Two new cases are red, and both bugs come from the unit's new claim and put-back. Two other new probes are green.

1. `git --no-pager diff --stat 23c816a73..54c1c0c73`
```
 .../postgres/replicas/adversary_w33_u3.rs          | 497 ++++++++++++++++++---
 1 file changed, 445 insertions(+), 52 deletions(-)
```
Only the test file changed. The mutant edit went into my own managed worktree, `spindle-w33-u3-adv3-mutant`, which is now archived and finished. Nothing under attack was edited.

2. **Cases** in `crates/spindle-cli/tests/spindle-cli/postgres/replicas/adversary_w33_u3.rs`. Each was first run alone (`adv3-scratch/alone.sh`, one slot job, one nextest call per case).

- **`a_reset_that_outlasts_the_lease_keeps_the_removed_components_lease_and_removes_its_record`** (rewritten): green, 6.7 s alone, 6.5–8.9 s under load.
  - **Why it failed before:** `serve.rs:793` releases the lease (`unclaimed`) before `exchange.respond`. The old standby polled `take` until the HTTP thread finished, so it could take the lease in that gap. That take is legitimate, and the old `(Some(3), None)` failure fits it.
  - **What it does now:** it reads audit's lease row every 10 ms without taking it. It fails if the row still holds the reset's token with an `until` at or before the server clock before the release (`until` 0) shows up. It also checks that the lease was held for more than 1.3 s.
  - **Smaller world:** 200 tasks instead of 1,500. A statement trigger sleeps 8 ms on each write to board's tables, so the reset outlasts the 1 s lease however loaded the node is. The call waits up to 40 s for an answer, so the 10 s answer bound no longer decides anything.
  - **Evidence:**
    - Head: green 5 of 5 (1 alone, 3 in parallel with the module, 1 in the full lane).
    - Mutant `adv3-scratch/mutant.patch` (renew of `claimed` removed from `Leases::renewing`): red 3 of 3. Verbatim: `audit's lease, taken by the reset at token 2, ran out 1.006 s after it was first read held, before the reset released it: another process could take it, and write a record beside the emptied store. Reading (token, until, clock) (2, 1795863625.556617, 1795863625.566561)`
- **`a_reset_refused_for_the_second_removed_components_lease_gives_the_first_back_and_keeps_both_records`**: green, 1.6 s. The reset answers 409 naming retired's table, gives audit's lease back, removes no record and empties no store. The next reset removes both records and holds neither lease.
- **`a_removal_failing_in_the_second_removed_components_table_puts_the_first_back_and_holds_no_lease`**: green, 1.4 s. The reset answers 500 naming retired's table, puts audit's record back, holds neither lease and empties no store.
- **`a_reset_refused_409_whose_put_back_fails_leaves_the_part_done_seed_to_resume`**: red, 2.4 s. Verbatim:
```
b10x-spindle: the row `seed` of spindle_test_nhczrcqnhlqp.spindle_audit_p_lease was not put back after a reset that emptied nothing, so a start with that seed takes a store with rows as seeded: store: PostgreSQL refused a statement on the lease of spindle_test_nhczrcqnhlqp.spindle_audit_p_lease: the write of the seed record failed (SQLSTATE P0001)
...the seed is taken as applied there, so no entry is sent, and the record now holds every entry done
  left: ["s1"]
 right: ["s1", "s2", "s3"]
```
- **`a_claim_that_outlasts_the_runs_wait_keeps_no_lease_and_removes_no_record_once_refused`**: red, 5.8 s alone, up to 7.6 s under load. Verbatim:
```
assertion `left == right` failed: the reset answered 500 {"refused":"the reset is refused before any store is emptied: a seed record in the lease table of a component this specification no longer has could not be removed: the lease thread did not answer about the seed's record within 2.666666664s"}, refused before any store was emptied, yet the run holds the leases listed after it and the records are as listed:
  left: (["…spindle_archive_p_lease", "…spindle_audit_p_lease", "…spindle_retired_p_lease", "…spindle_vintage_p_lease"], [None, None, None, None])
 right: ([], [Some({"digest": …, "done": 2, …}) ×4])
```
Times over 5 s: the renew case (6.5–8.9 s) and the claim case (5.8–7.6 s).

3. **Suite runs**, after the cases existed:
- `sh adv3-scratch/module.sh` (the module ×3, one slot job): each round `8 tests run: 6 passed, 2 failed`. The same two are red every time.
- `slot --timeout 30m --tokens 4 task check:postgres`: `Summary [ 95.263s] 194 tests run: 192 passed, 2 failed, 1510 skipped`; `within: 96.036 s of 100 s, the command failed with status 100`; exit 201. The only failures are the two red cases.
  - The lane is close to its 100 s bound. In the implementor's gate run it took 153 s, which was over the bound.
  - The rewritten case dropped out of the ten slowest (10.6 s before, 6.5 s now).
- `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings`: exit 0. `cargo fmt -p spindle-cli --check`: clean.

4. **Findings**, covering 54c1c0c73:

| # | file:line | what was measured | what reaches it | verdict | origin |
|---|---|---|---|---|---|
| F1 | `crates/spindle-cli/src/serve.rs:777` | When the claim outlasts the run's wait (`SeedRow::asked`, `database.rs:952`, about 2.67 s with a 2 s lease and 2 components), the reset answers 500 "refused before any store is emptied", and `seed::claimed`'s doc says no lease is kept. But the refusal path `continue`s without `unclaimed`, and the lease thread finishes the claim anyway. Result: every orphan record is removed, every orphan lease stays held and renewed until the next reset (whose claim releases them without putting anything back) or until the run exits, and the store stays full. A seeded start then counts the seed as applied, and release 1 cannot lead those components. | A claim slower than (components + 2) × statement wait: a slow or locked database, or many removed components' tables. The test builds it with a 0.4 s trigger sleep on each of 8 statements. Each statement stays under its own timeout, so the only thing exceeded is the run's wait. | CONFIRMED (warning) | introduced |
| F2 | `crates/spindle-cli/src/database.rs:737` | After a reset refused 400/409 (no store emptied), a put-back that fails only logs a warning. The part-done record (1 of 3) is gone while the store still holds entry 1, so the next seeded start counts the seed as applied (D5 B) and never sends entries 2 and 3. Before this unit, a refused reset never touched the record. | A failed write of the `seed` row at put-back time. The test uses a trigger. In practice a lock or statement timeout on the lease connection could cause it, which is the same kind of failure as the one round 2 had fixed. | CONFIRMED (warning) | introduced |

- **Fix for F1:** on a claim error, ask `Unclaim(true)` before answering. It queues behind the claim, so it runs after the claim finishes, then puts the records back and releases the leases. Or make the claim's wait cover its own statements.
- **Fix for F2:** the trade-off is built into the order chosen (remove first, put back after). One cheap step is to make the answer report the loss (500 naming the table) rather than a plain 409.

Judgement, no case written: a crash between the claim's removal and the emptying leaves "rows and no record" (D5 B), and a part-done seed's progress is lost. Removing after the reset has the mirror-image window, so I'm not raising it as a finding.

5. **Attacked and could not break:**
- A second removed component's lease held by another process: 409, the first lease is given back, both records kept.
- A 500 halfway through several removed tables: earlier records put back, both leases released, nothing emptied.
- Leases left held after a 200, a 400 (round 1 case), a claim 409 or a claim 500: none.
- 409/400 from `console::reset` after something was emptied: unreachable. `HostError::World`/`Clock` are raised before `empty()`, and the `step_clock` check after loading cannot fail with a manual clock.
- The renewal being starved under load: no expiry seen in 5 runs on head.

6. **Paths written outside the worktree:**
- `<local path>`, containing `head.rs`, `top.rs`, `tail.rs`, `alone.sh`, `alone.log`, `mutant.sh`, `mutant.log`, `mutant.patch`, `module.sh`, `module.log`, `lane.log`, `clippy.log`, `commit.txt`
- Mutant worktree `<local path>`, now finished
- Its archive at `<local path>`

7.
```findings
- file: crates/spindle-cli/src/serve.rs
  line: 777
  category: concurrency
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a claim that outlasts SeedRow::asked's wait is answered 500 "no lease is kept" without unclaimed, while the lease thread goes on to remove every orphan record and keep every orphan lease with the store left full
- file: crates/spindle-cli/src/database.rs
  line: 737
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a failed put-back after a reset refused 400/409 only warns, so a part-done orphan record is lost while no store was emptied and the next seeded start skips the remaining entries
```

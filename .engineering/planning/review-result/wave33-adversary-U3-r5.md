---
format: aep.planning-md/3
id: review-result:wave33-adversary-U3-r5
kind: review-result
status: active
title: Adversary, round 5, on wave 33 U3
relations:
- reviews: story:seed-record-after-a-component-is-removed
revision: 1
---
```
unit: story:seed-record-after-a-component-is-removed, findings cover 6887f5974 (tests committed as 597a9bbbd on unit/w33-orphan-seed)
verdict: CONFIRMED
cases: executed 198→200, red 2
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 4 paths (part 6)
needs-coordinator: none
```

I found two defects. Both happen when the run loses its lease during a reset and the lease thread has ended before the reset asks for the put-back. Both fail in the lane, each in about 3 to 4 s. The 198 is the 200 cases of the lane run after my additions, minus my two; I did not make a separate run with them left out.

**1. Diff**

```
 .../postgres/replicas/adversary_w33_u3.rs          | 173 +++++++++++++++++++++
 1 file changed, 173 insertions(+)
```

It touches only the test module. Helpers added there: `store_tables`, `held_for`, `taken_over`, `waiting_on`.

**2. Cases added** (in `<local path>`, both red now)

Both cases use a lease of 1 s. A test session holds board's store tables `ACCESS EXCLUSIVE` for 2.5 s, and board's lease token is moved on while the reset waits for those tables. The run's lease thread sees the takeover at its next renewal.

- **Case 1:** `a_lost_lease_during_a_reset_with_no_removed_component_reports_no_lost_seed_record`
  - Setup: an ordinary leased run, with no lease table of a removed component in the schema.
  - Asserts: the answer does not say that a removed component's seed record was removed.
  - Red output, run alone (3.071 s):
    ```
    no lease table of a removed component exists and the claim removed no record, yet the reset that lost the lease answers 500 that such a record was removed and not put back (exit ExitStatus(unix_wait_status(256))): resetting board: reading tasks.board.Task: store: event store deadline exceeded during database statement or lock; its store could not be reopened, and every read and write of it now fails; the reset changed no store's rows, yet the seed record removed for it from the lease table of a component this specification no longer has was not put back, as far as this run knows: the lease thread has ended, so the seed's record is not read or written; a start with that seed may take the store as seeded and skip its entries, so reset the store before such a start
    ```
- **Case 2:** `a_lost_lease_during_a_reset_slower_than_a_term_still_puts_the_removed_components_record_back`
  - Setup: audit's record is at `done` 1 of 3, and the claim removes it.
  - Asserts: when the run exits, audit's record is back, as `CHANGELOG.md:94` says: "a run that lost its lease puts them back before it exits".
  - Red output, run alone (3.461 s):
    ```
    assertion `left == right` failed: the reset that lost the lease answered 500 and the run exited (ExitStatus(unix_wait_status(256))) without putting audit's record back: {"refused":"resetting board: reading tasks.board.Task: store: event store deadline exceeded ...; the reset changed no store's rows, yet the seed record removed for it ... was not put back, as far as this run knows: the lease thread has ended, ..."}
      left: None
     right: Some(Object {"digest": String("sha256:257f…"), "done": Number(1), "rejected": Array []})
    ```

**3. Commands run after the cases existed**

- `slot --timeout 30m --tokens 4 task check:postgres`, exit 201:
  - `Summary [ 128.643s] 200 tests run: 198 passed (1 slow), 2 failed, 1511 skipped`
  - The two failures are case 1 (2.965 s) and case 2 (3.728 s).
  - `within: 129.589 s of 100 s`. The overrun comes from the kernel scan cases (the slowest is `adv2_a_scan_answers_over_a_million_events` at 40.8 s), which `story:postgres-lane-margin` is fixing. It is not from these cases.
- `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings`: `Finished`, no warnings.
- `cargo fmt -p spindle-cli --check`: clean.

**4. Findings**

- **F1** `crates/spindle-cli/src/seed.rs:391`, called at `serve.rs:801`. CONFIRMED, introduced.
  - **What was measured:** case 1. The claim answered `Claim::Taken(0)`, but `unclaimed` still sends `Ask::Unclaim` when `tables == 0`. If the lease thread has already ended (`closing` returns at once when `claimed` is empty), the answer says that a removed component's seed record "was not put back" and tells the operator to reset the store. At base, a run like this answered the plain lease-lost refusal.
  - **What reaches it:** any leased deployment, even one without removed components, whose lease is taken while a reset waits on its store tables, here a lock wait of up to 2 s.
  - **Fix:** return `answer` unchanged in `unclaimed` when `tables == 0`.
- **F2** `crates/spindle-cli/src/database.rs:881-882` (`Leases::closing`), against `CHANGELOG.md:94` and the "puts the rows back … before it answers" sentence in `environment-and-admission.md`. CONFIRMED, introduced.
  - **What was measured:** case 2. After the lease is lost, `closing` waits at most one term, and it does not renew the leases the claim took. A reset that is still waiting after that gets no put-back, and the record stays lost while the store still holds entry 1. The next seeded start then takes the store as seeded (D5 B), and entries 2 and 3 are never sent: this is round 4's defect again, past one term.
  - The answer itself is honest ("as far as this run knows"), but it names no table. The docs say a failed put-back names the tables whose record was lost.
  - **What reaches it:** a takeover during a reset that waits longer than one term, which the store's 2 s lock wait allows on a lease of 1 to 6 s.
  - **Fix:** either renew the claimed leases inside `closing` and wait for the reset's `Unclaim` as long as the reset may take, or state the one-term bound in the CHANGELOG and docs and name the tables in the answer.

**5. Attacked, could not break**

- `reset_changed_rows`, by reading the code:
  - It is reset at the start of `reset_to`, so a stale value is not reachable, because `refused_world` repeats every refusal `console::reset` makes before `reset_to`.
  - A partial empty, a failing world row, a `forget_events` failure, memory and file stores, and a reset that forgot only events each match its doc comment.
- The statement counts for the claim and the put-back match the statements they run. Each statement is bounded by `statement_timeout` and `lock_timeout`, so a lock wait cannot outrun the wait.
- The process does not exit before the put-back: `Kept::drop` joins the lease thread, and `Unclaim` is queued before `Stop`.
- A second reset during the put-back: the serve loop runs resets one at a time, and another process's reset needs the orphan lease, so it is refused 409.

**6. Paths written outside the worktree**

- `<local path>`
- `<local path>`
- `<local path>`
- `<local path>`

**7. Findings block**

```findings
- file: crates/spindle-cli/src/seed.rs
  line: 391
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a reset whose claim removed no record (Claim::Taken(0)) still asks the lease thread for a put-back, and after a lost lease answers that a removed component's seed record was not put back and the store must be reset, where the base answered the plain lease-lost refusal
- file: crates/spindle-cli/src/database.rs
  line: 881
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: Leases::closing waits one term without renewing the claimed leases, so a reset that outlasts it after a lost lease exits with the removed component's record gone, against the CHANGELOG's "puts them back before it exits", and the answer names no table
```

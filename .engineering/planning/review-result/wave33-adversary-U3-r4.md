---
format: aep.planning-md/3
id: review-result:wave33-adversary-U3-r4
kind: review-result
status: active
title: Adversary, round 4, on wave 33 U3
relations:
- reviews: story:seed-record-after-a-component-is-removed
revision: 1
---
unit: story:seed-record-after-a-component-is-removed (wave 33 U3), head `dbbf08a06`; findings cover `dbbf08a06` with my tests committed on top as `4b17f98dc`
verdict: CONFIRMED
cases: executed 196→198, red 2
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 5 paths (part 6)
needs-coordinator: none

Two new ways to break it, both red on the unit's head. The serious one: a reset that the lease's fence refuses (`503`) after the claim has run empties no store, but the removed component's seed record is gone. The next start with the same seed then skips the entries the seed had not sent yet. The second is a wrong message: when putting the records back takes longer than the run waits, it answers `500` saying they are lost, and they are put back right after. Rounds 1–3's cases are all green now. No timing flake this run.

1. `git --no-pager diff --stat dbbf08a06..HEAD` (the tree is clean after my commit):
```
 .../postgres/replicas/adversary_w33_u3.rs          | 126 +++++++++++++++++++++
 1 file changed, 126 insertions(+)
```
It touches only the test file. No implementation file changed.

2. Cases added to `<local path>`. Each was run alone first, as one lane job:

- **`a_reset_fenced_after_the_claim_empties_no_store_and_leaves_the_part_done_seed_to_resume`** — red, 1.35 s alone, 3.3 s in the lane.
  - Setup: release 1 is stopped with `audit`'s record at `done` 1 of 3. A trigger on `audit`'s lease table moves `board`'s lease token on, inside the claim's own delete. That is a deterministic stand-in for another process taking over the lease at that moment.
  - What happens: the reset answers `503`; the fence lands no write.
  - Assertion: the next seeded start of release 2 sends s2 and s3. It does not:
```
reset 503 {"refused":"the run lost the lease of its store to another process and exits"}
next start printed:
b10x-spindle: seed …/seed.json (sha256:257f06f9…): the PostgreSQL store holds 1 rows and the row `seed` of spindle_test_lnmdelmrtukb.spindle_board_p_lease records no seed, … the seed is taken as applied there, so no entry is sent, and the record now holds every entry done
  left: ["s1"]
 right: ["s1", "s2", "s3"]
(audit's table holds None)
```
- **`a_put_back_that_outlasts_the_runs_wait_is_not_answered_as_lost_once_it_lands`** — red, 5.1 s alone, 6.9 s in the lane (over 5 s).
  - Setup: six removed components' tables (`audit` plus five copies). A trigger holds each write of a row `seed` 0.5 s, which is under the statement timeout of 0.67 s. A world whose rows are refused `400` comes after the claim.
  - What happens: the put-back takes about 3 s, past the run's wait of 2.67 s.
  - Assertion: if the answer says "is lost", the records are gone. They are not:
```
{"refused":"the reset is refused before any store is emptied (400 …), yet the seed record removed for it … is lost: the lease thread did not answer about the seed's record within 2.666666664s; … so reset the store before such a start"}
after it every table holds {"digest":"sha256:0377…","done":2,"rejected":[]} again: [Some(…) ×6]
```

3. Suite run, after the cases existed: `slot --timeout 30m --tokens 4 task check:postgres`
```
Starting 198 tests across 3 binaries (1510 tests skipped)
Summary [  33.892s] 198 tests run: 196 passed, 2 failed, 1510 skipped
  FAIL [   3.256s] … adversary_w33_u3::a_reset_fenced_after_the_claim_empties_no_store_and_leaves_the_part_done_seed_to_resume
  FAIL [   6.947s] … adversary_w33_u3::a_put_back_that_outlasts_the_runs_wait_is_not_answered_as_lost_once_it_lands
within: 34.351 s of 100 s, the command failed with status 100
exit 201
```
- The "before" figure of 196 is this run's 198 minus my two cases, not a separate run.
- Also run: `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings` finished clean, and `cargo fmt -p spindle-cli --check` exited 0.

4. Findings, on tree `4b17f98dc`:
- **Fenced reset loses the record.** `crates/spindle-cli/src/serve.rs:790`, `let untouched = matches!(answer.status, 400 | 409);`. A reset that ends in `503` is never treated as untouched, so the claim's removal stands although no store was emptied.
  - This is inconsistent with a record in a current table: `forget_row` runs only on `200`, so a fenced reset keeps that one.
  - What reaches it: a leader that loses its lease (paused, or its session ends) while an operator resets. That is the documented D4 path.
  - A `500` from `console::reset` that fails on its first store write (nothing emptied) gives the same result. I did not write a separate case for it.
  - Origin introduced: at `114cd03ca` (orphan read, no claim) a reset forgets only on `200`, so the record stays and the seed resumes.
  - Possible fix: put the records back on any answer where no store was emptied (`503` from the fence, `500` from the first store write), or remove the orphan record only after the reset answered `200`.
- **"Lost" when it is not.** `crates/spindle-cli/src/seed.rs:386`, `Err(error) if untouched`. A timeout of the lease thread is reported as a loss, yet the queued put-back still lands.
  - What reaches it: a put-back slower than four statement timeouts, i.e. many removed tables or lock waits.
  - The answer tells the operator to reset the store when nothing was lost. The CHANGELOG line "no record is lost" also reads differently from this answer.
  - Origin introduced (the put-back is the unit's).

5. Attacked and not broken:
- **Queued undo after a claim timeout.** The undo goes first-in-first-out ahead of the next reset's claim. The round-3 case `a_claim_that_outlasts_the_runs_wait…` is green.
- **Checks moved before the claim.** `refused_world` matches every `400`/`409` that `Host::reset_to` gives before emptying. After emptying, `reset_to` gives only `500`; a late clock `409` cannot happen because the clock is manual.
- **Token lost between take and removal, and in the put-back.** The single-transaction token-and-session check refuses both and changes no row.
- **Leases after a claim refused `409`, a `500` removal, or a fenced run.** None are left held: the dead session makes them free to take.
- **CHANGELOG and admission page against the code**, apart from the drift above.

6. Paths written outside the worktree, all under `<local path>`:
- the directory itself
- `case1.log`
- `case2.log`
- `lane.log`
- `commit.txt`

```findings
- file: crates/spindle-cli/src/serve.rs
  line: 790
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a reset the lease's fence refuses (503) after the claim empties no store but leaves the removed component's record deleted, so the next seeded start takes the store as seeded and never sends the remaining entries (adversary_w33_u3.rs:872)
- file: crates/spindle-cli/src/seed.rs
  line: 386
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a put-back that outlasts the run's wait for the lease thread is answered 500 "is lost" and tells the operator to reset the store, while the queued put-back then restores every record (adversary_w33_u3.rs:938)
```

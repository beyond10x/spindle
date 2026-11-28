---
format: aep.planning-md/3
id: review-result:wave31-adversary-U6-r4
kind: review-result
status: active
title: Adversary, round 4, on wave 31 U6
relations:
- reviews: story:several-writers-one-postgres
revision: 1
---
I amended the case and committed it as f67f80c9 on unit/w31-writers, on top of 6c611de9. The case passed when run alone, but that run included the implementor's uncommitted round-4 work, so it does not cover 6c611de9 alone.

unit: story:several-writers-one-postgres, round-3 amendment, head f67f80c9; the run covers the working tree including uncommitted round-4 changes
verdict: nothing found
cases: executed 1→1 (the amended case alone), red 0
origin: introduced 0 / pre-existing 0 / undecided 0
wrote-outside-worktree: 3 paths (part 6)
needs-coordinator: the run included the implementor's uncommitted round-4 changes (part 3)

**1. Diff stat (6c611de9..HEAD).** One test file, no other path.
```
 .../tests/spindle-cli/adversary_w31_u6.rs          | 74 +++++++++++++++++-----
```

**2. What the amended case asserts** (`a_dropped_leader_session_fails_over_with_each_write_once`)
- The write-once checks are unchanged:
  - The old leader exits 1 and logs that it writes nothing more.
  - Every command it answered 201 is held, and none it answered 503 landed.
  - Every answer is 201, 503 or no answer.
  - The standby leads.
- New: every task held after the failover, plus a new task `n1` opened on the new leader, is posted at least once, waiting up to 30 s.
- New: each repeated post carries the same `(component, log, position)` as that task's first post.
- Removed: the "no repeat at all" check.

**3. Run**
- `slot --timeout 30m sh ./.pglane.run.sh cargo nextest run --locked --profile postgres -p spindle-cli --test spindle-cli --run-ignored all --success-output immediate -E 'test(~adversary_w31_u6::postgres::a_dropped_leader)'`, exit 0:
  ```
  PASS [   2.083s] (1/1) spindle-cli::spindle-cli adversary_w31_u6::postgres::a_dropped_leader_session_fails_over_with_each_write_once
  the old leader answered 29 commands: 28 x 201, statuses [("f29", 503)]
  30 posts of 29 tasks, 1 repeats, each naming its first post's event
  ```
- `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings`: exit 0.
- `cargo fmt -p spindle-cli --check`: exit 0.
- **Caveat:** the worktree held uncommitted changes from the implementor's round 4 in `crates/spindle-cli/src/database.rs`, `crates/spindle-kernel/src/postgres.rs` and `website/docs/concepts/environment-and-admission.md`. The run built them, so the pass covers that working tree and is not a result for 6c611de9 by itself. I did not touch or commit those files; my commit stages only the test file.

**4. Findings:** none new. With this case amended, the egress finding from round 3 (`crates/spindle-cli/src/egress.rs:354`) is settled by your decision. The standby-drop finding (`crates/spindle-cli/src/database.rs:332`) still stands as reported in round 3.

**5. Not broken:** across the failover the repeats are identical `(component, log, position)` and nothing is lost (1 repeat in this run, up to 5 in round-3 runs).

**6. Paths written outside the worktree** (all under `<local path>`)
- `r3-amend.log`
- `r3-amend-clippy.log`
- `commit4.txt`

**7. Findings block**
```findings
[]
```

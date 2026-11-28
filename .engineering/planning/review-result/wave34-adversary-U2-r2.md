---
format: aep.planning-md/3
id: review-result:wave34-adversary-U2-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 34 U2
relations:
- reviews: story:unknown-commit-answered-as-unknown
revision: 1
---
```
unit: story:unknown-commit-answered-as-unknown, head 128de4b8 (findings cover 81777c19)
verdict: CONFIRMED
cases: executed 265→268, red 0
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 6 paths (scratch logs, commit message, probe worktree, part 6)
needs-coordinator: probe worktree w34-u2-adv-r2-mutant is finished and clean; gc keeps it until 81777c19 is on a remote ref, so remove it after the push
```

**1. Diff stat** (`git --no-pager diff --stat 81777c19..HEAD`). Both paths are test files.
```
 .../tests/spindle-host/adversary_w34_u2_r2.rs      | 234 +++++++++++++++++++++
 crates/spindle-host/tests/spindle-host/main.rs     |   2 +
```
Committed as 128de4b8 through `b10x-gates bot`. The private-string grep printed nothing.

**I could not break the fix.** All three cases I wrote are green at head. One of them catches a mutant that the rest of the suite does not.

**2. Cases** in `<local path>`:
- `a_world_row_refused_busy_on_an_empty_store_changed_no_rows`: the store is empty and the only world row's commit fails `Busy` before it lands. Then `reset_changed_rows()` must be `false`. Green.
- `forgetting_events_unknown_on_an_empty_store_loses_it_and_changed_no_rows`: `forget_events` fails `Unknown`. The result must be `Reset { restored: false }`, the reading `false`, and later commands must fail with "lost in a reset". Green.
- `an_unknown_removal_leaves_no_reading_for_the_next_reset`: a removal ending `Unknown` reads `true`. The next reset, over the store that was lost, reads `false`. Green.

Run alone with `cargo test --locked -p spindle-host --test spindle-host adversary_w34_u2_r2`: `test result: ok. 3 passed; 0 failed; ... 279 filtered out; finished in 0.00s`. Each case takes under 0.01 s.

Mutant probe, run in a separate managed worktree; the unit tree was not touched. At `host.rs:1952` the condition `matches!(error, StoreError::Unknown(_))` was replaced by `true`:
- `cargo test --locked -p spindle-host --no-fail-fast -- --skip adversary_w34_u2_r2` → `test result: ok. 265 passed; 0 failed; 14 ignored; ... 3 filtered out`. The existing suite misses the mutant.
- With my case included, it fails: `panicked at .../adversary_w34_u2_r2.rs:160:5: a world row refused before it landed, on a store with nothing to remove, changed no row`.

**3. Suite at head, after the cases existed**
- `cargo test --locked -p spindle-host --no-fail-fast` → `test result: ok. 268 passed; 0 failed; 14 ignored; ... finished in 10.21s`, exit 0.
- `cargo clippy --locked -p spindle-host --all-targets -- -D warnings` → exit 0.
- `cargo fmt -p spindle-host --check` → exit 0.
- The "before" count of 265 comes from the probe-tree run with my 3 cases skipped.

**4. Findings that are not failing cases** (on 81777c19)
- `crates/spindle-host/src/host.rs:1951`, mutant, warning, CONFIRMED, introduced. The fix only counts an unknown outcome as a changed row. Before my case, no test checked the other side: a world row that failed without landing must leave the reading `false`. Without that check, a fix that counted every failed row would stop the leased reset in `crates/spindle-cli/src/serve.rs:796` from putting the seed records back. My case now covers this, so the finding is closed by the kept test.
- `crates/spindle-kernel/src/store.rs:366`, contract-drift, note, CONFIRMED, introduced. The `# Errors` docs of `Store::commit`, `remove`, `commit_all` and `commit_events` (lines 366, 376, 396) still name only `Conflict` and `Backend`. They don't mention `Unknown`, which may have written. Fix: one line per method. (`Busy` is missing from them since wave 33.)

**5. Attacked and not broken**
- **Other places that set or read the flag:** `reset_rows` is set in `empty`, `load` and `removed_all`, and read only at `serve.rs:796`. Nothing else matches on `StoreError::Unknown`.
- **Flag carried from one reset to the next:** `reset_to` clears it on its first line. In `console::reset`, every refusal that comes before `reset_to` is caught earlier by `seed::refused_world`, so a stale reading is never used.
- **`serve.rs` restore after an `Unknown` removal:** `restore` is false and the leases are released without putting the records back. The 503 lease-lost replacement at `:802` would need a fence refusal in the same reset. A fence refusal stops the reset, so I found no path that reaches it.
- **Kernel mapping:** every PostgreSQL commit, remove and forget path that can return eventlog's `UnknownCommit` goes through `backend()`. The other `StoreError::Backend(...)` sites build their own text.
- **Doc claims:** the CHANGELOG, `reset_changed_rows`, `removed_all`, `ExecuteError`, `HostError::Execute`, `Key::Failed` and `Failure::Store` match the code. `ExecuteError::Store(_)` maps to `Failure::Store` (`telemetry/src/kernel.rs:26`).

**6. Paths written outside the worktree**
- `<local path>` — `cases.log`, `mutant.log`, `suite.log`, `mutant-suite.log`, `clippy.log`, `commit-msg.txt`
- `<local path>`: the probe worktree, made with `worktree create`. Its mutation is reverted, it is finished and its build cache discarded. The coordinator removes it after the push.

**7. Findings**
```findings
- file: crates/spindle-host/src/host.rs
  line: 1951
  category: mutant
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: no case held that a world row failing without landing leaves reset_changed_rows false, so the mutant counting every failed row passed all 265 cases until adversary_w34_u2_r2 added one
- file: crates/spindle-kernel/src/store.rs
  line: 366
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the Errors docs of Store::commit, remove, commit_all and commit_events name only Conflict and Backend, not the new Unknown that may have written
```

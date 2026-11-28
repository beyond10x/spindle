---
format: aep.planning-md/3
id: review-result:wave34-adversary-U3-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 34 U3
relations:
- reviews: story:reset-on-a-busy-store-keeps-the-store
revision: 1
---
```
unit: story:reset-on-a-busy-store-keeps-the-store, findings cover 483a8cff (tests committed as daee7976)
verdict: CONFIRMED
cases: executed 269→270, red 1
origin: introduced 0 / pre-existing 1 / undecided 0
wrote-outside-worktree: 3 paths
needs-coordinator: none
```

Round 2 found one small defect, a missing name in an error message. The fix in 483a8cff holds: no store call is missing from the implementor's list, and I found no Busy error that turns into another error on its way to `console::reset`. The one red case sits outside the fix.

**1. Diff stat** (483a8cff..HEAD; every path is a test file)
```
 .../tests/spindle-host/adversary_w34_u3_r2.rs      | 166 +++++++++++++++++++++
 crates/spindle-host/tests/spindle-host/main.rs     |   2 +
 2 files changed, 168 insertions(+)
```

**2. Case added**, in `<local path>`
- `a_busy_world_row_write_names_the_component_and_the_step`, **red**, under 0.01 s.
- It sends a reset with a world. The reset first removes the existing row, then the store answers busy when the world's row is written.
- It asserts that the error text names `desk` and `row 0`. Decision D2 A asks a busy reset to name the component and the failed step.
- Its first run, the case alone (`cargo test --locked -p spindle-host --test spindle-host -- adversary_w34_u3_r2`):
```
thread 'adversary_w34_u3_r2::a_busy_world_row_write_names_the_component_and_the_step' panicked at crates/spindle-host/tests/spindle-host/adversary_w34_u3_r2.rs:160:5:
a busy reset names the component and the step (D2 A), as the world check's read does ("resetting desk: reading row 0, ..."); the world row's write answers only the store's own text: "event store deadline exceeded during database statement or lock"
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 281 filtered out; finished in 0.00s
```

**3. Suite run, after the case existed**: `cargo test --locked -p spindle-host --no-fail-fast`, exit 101, 17 s wall time.
```
unittests src/lib.rs:        test result: ok. 2 passed; 0 failed; 0 ignored
tests/spindle-host/main.rs:  test result: FAILED. 267 passed; 1 failed; 14 ignored; finished in 10.32s
```
The one failure is the new case. "Before" is 269: this run with my one case taken out. `cargo clippy --locked -p spindle-host --all-targets -- -D warnings` passes, and `cargo fmt -p spindle-host --check` exits 0.

**4. Finding**
- **What was measured:** `crates/spindle-host/src/host.rs:1956-1963` (`Host::load`). The world row is written with plain `commit(...)?`, so a busy error keeps only the store's own text. The console's `503` or `500` then names no component and no step, so with several stores nobody can tell which one to wait for.
- **What reaches it:** `POST /api/reset` with a world body on a PostgreSQL store, held behind another session's lock after the stores were emptied.
- **Verdict:** CONFIRMED.
- **Origin:** pre-existing. `git show 2577c62b:crates/spindle-host/src/host.rs` shows `Host::load` unchanged at the base, so the case fails there too. Decision D2 A of this story still asks for the name. The coordinator decides whether it stays in this wave.
- **Fix, named only:** in `Host::load`, map `StoreError::Busy(detail)` to `"resetting {owner}: writing row {index}, {entity} {identity}: {detail}"`, the same form the world check's read uses.

**5. Attacked and not broken**
- Every wrapper between the host and the store (`Counted`, `CountingStore`, `Supplied`) passes the store's error through unchanged.
- The world check makes no other store call (`resting`, `store_of`, `clock_at`). `hold` and `publish` cannot fail, and `publish` runs on every return.
- `last_dead_letter_id` and `continue_dead_letter_ids` run only for file and memory stores, never for a supplied one.
- The PostgreSQL store's `remove`, `forget_events` and `commit_all` only answer busy before a write starts, so no removal lands while the reset reports it as busy.
- The marker after `Host::reset` is lost has no effect on a supplied store, so a change to it cannot be observed.
- The serve paths (`reset_seeded`, `--state`, leased, router) only remap a busy answer after a lost lease or a failed put-back. Both are correct.
- I did not test a host that mixes a memory store and a supplied one. Building that environment is out of reach for these fixtures. By the code, an empty memory store emptied earlier makes a busy reset answer `500`, not `503`.

**6. Paths written outside the worktree**
- `<local path>`
- `<local path>`
- `<local path>`

**7. Findings**
```findings
- file: crates/spindle-host/src/host.rs
  line: 1956
  category: acceptance
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: a reset's world-row write held busy answers HostError::Store(Busy) with the store's raw text, naming neither the component nor the step that D2 A asks a busy reset to name
```

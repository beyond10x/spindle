---
format: aep.planning-md/3
id: review-result:wave34-adversary-U2-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 34 U2
relations:
- reviews: story:unknown-commit-answered-as-unknown
revision: 1
---
unit: story:unknown-commit-answered-as-unknown, head 65e7f2a9 (findings cover 454f1413, which only adds my tests)
verdict: CONFIRMED
cases: executed 264→267, red 3
origin: introduced 1 / pre-existing 4 / undecided 0
wrote-outside-worktree: 3 paths (part 6)
needs-coordinator: none

The unit's own job holds: every path that answers a command store error gives `500` `{"refused", "committed": "unknown"}`. The three red cases all come from a reset that meets an unknown commit. Two show the host still reports that the reset changed no rows. The third shows the story's CHANGELOG bullet claims more than the code does. None of them is a blocker.

**1. `git --no-pager diff --stat 65e7f2a9 HEAD`**
```
 .../tests/spindle-host/adversary_w34_u2.rs         | 207 +++++++++++++++++++++
 crates/spindle-host/tests/spindle-host/main.rs     |   2 +
 2 files changed, 209 insertions(+)
```
Both are test files, committed as 454f1413 with `b10x-gates bot`. I did not take the worktree session lease, and I hold none to release.

**2. Cases added** in `<local path>`, plus its `mod` line in `main.rs`. The stub store applies the write, then returns `StoreError::Unknown`, which is what happens when a COMMIT landed and the deadline fired anyway. Each case takes under 0.01 s.
- `a_reset_whose_first_removal_is_unknown_may_have_changed_rows`: the removal landed (the store is empty), so `reset_changed_rows()` should be true. It is false. **Red.**
- `a_reset_whose_world_row_is_unknown_may_have_changed_rows`: the world row landed, so `reset_changed_rows()` should be true. It is false. **Red.**
- `a_reset_whose_world_row_is_unknown_loses_the_store`: the CHANGELOG says "a reset that meets it loses the store". Here the store stays in use. **Red.**

Output of `cargo test --locked -p spindle-host --test spindle-host adversary_w34_u2`, run before the suite:
```
panicked at .../adversary_w34_u2.rs:146:5:
a removal whose outcome is unknown may have changed rows, yet the host reports the reset changed none, so a caller puts back what it removed for the reset: resetting desk: removing desk.notebooks.Notebook "ab381d27-…": store: commit outcome unknown; reconnect and resolve the same command identity; its store could not be reopened, and every read and write of it now fails
panicked at .../adversary_w34_u2.rs:172:5:
a world row whose outcome is unknown may have changed rows, yet the host reports the reset changed none: store: commit outcome unknown; reconnect and resolve the same command identity
panicked at .../adversary_w34_u2.rs:198:5:
a reset that met an unknown outcome (store: commit outcome unknown; …) loses the store, as the CHANGELOG says; the next command answered Ok(1) and the store holds 2 rows
test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 276 filtered out; finished in 0.00s
exit 101
```

**3. Suite run** with `cargo test --locked -p spindle-host --no-fail-fast`, exit 101:
```
unittests: test result: ok. 2 passed; 0 failed; 0 ignored
tests/spindle-host/main.rs: test result: FAILED. 262 passed; 3 failed; 14 ignored; finished in 10.31s
```
The only failures are my three cases. "Before" is 264: the 267 cases that ran, less my 3.

**4. Findings** (covering 454f1413)
1. `crates/spindle-host/src/host.rs:3032` (`removed_all`) — CONFIRMED, pre-existing, note.
   - **What was measured:** case 1. The removal's `?` returns before `*removed = true`, so a removal that may have landed leaves `reset_changed_rows()` false. Its doc at `:2035-2041` treats that case as a refused removal.
   - **What reaches it:** only `crates/spindle-cli/src/serve.rs:796`, the leased reset that puts back seed records of a removed component (`restore = … && !host.reset_changed_rows()`). That needs a PostgreSQL deadline firing after COMMIT on the reset's first removal. It is a narrow path.
   - **Fix:** set the flag on `StoreError::Unknown` as well.
   - **Why pre-existing:** host source is unchanged by the unit. At base, `UnknownCommit` became `Backend` and took this same path.
2. `crates/spindle-host/src/host.rs:1946` (`load`) — CONFIRMED, pre-existing, note.
   - **What was measured:** case 2. A world row whose commit outcome is unknown leaves `reset_rows` false.
   - **What reaches it:** the same `serve.rs:796` path, when the reset carries a world.
3. `CHANGELOG.md:98` — CONFIRMED, introduced, note (contract drift).
   - **What was measured:** case 3. "a reset that meets it loses the store, as before" holds for a removal or for forgetting events on a supplied store. A world-row commit with an unknown outcome returns `HostError::Store` (`500` from `console::reset`), and the store stays usable.
   - **Fix:** either change the wording to "a reset whose removal meets it", or make the world-row path lose the store too. The coordinator picks one; case 3 asserts the current wording.
4. `crates/spindle-kernel/src/lib.rs:187` — CONFIRMED, pre-existing, note.
   - `ExecuteError`'s doc says "Nothing was written". `ExecuteError::Store(StoreError::Unknown)` is exactly the case where something may have been written, and it is the variant the ingress now matches. The `HostError::Execute` doc (`host.rs:350`) says "did not execute" in the same way.
5. `crates/spindle-telemetry/src/catalogue.rs:69` — CONFIRMED, pre-existing, note.
   - The counter `command.<Command>.failed.store` is documented as "the command did not execute". It also counts commands whose commit outcome is unknown.

**5. Attacked, could not break**
- **The `answer.rs` match arm:** a command store error from `commit`, `commit_all` or `commit_events` returns `HostError::Execute`, which the `Store(Unknown)` arm answers. The unit's tests catch any change to that arm, to the status or to the body.
- **`backend()` mapping:** every eventlog error in `sqlite.rs` and `postgres.rs` goes through `backend()`. Read paths never produce `UnknownCommit`, and no SQLite path that spindle calls does either.
- **Leased run in `serve.rs:814-817`:** `fenced()` cannot be set by the same request whose write returned Unknown. The fence refuses before COMMIT, and egress cursors are saved on the run's thread.
- **Listener check:** `as_bool` on `committed` reads `"unknown"` as not uncommitted. Changing it to `!= Some(true)` is caught by `an_unknown_commit_is_not_read_as_uncommitted`.
- **Request log class:** the answer is recorded `failed` with the `5xx` label.
- **Other places that read `committed`:** none in the repo — not ui, node, wasm, embed or calls. Embed uses the same `answering` path.
- **Out of scope, not reported:** dead-letter replay (`console::replay` answers `result: "failed"` and keeps the letter again) is a bound-command retry, so it belongs to `story:unknown-commit-in-seed-and-retries`.

**6. Paths written outside the worktree**
- `<local path>`
- `<local path>`
- `<local path>`

```findings
- file: crates/spindle-host/src/host.rs
  line: 3032
  category: acceptance
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: a reset removal that fails StoreError::Unknown after landing leaves reset_changed_rows false, so the leased reset in serve.rs puts the seed records back although rows may have changed
- file: crates/spindle-host/src/host.rs
  line: 1946
  category: acceptance
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: a world row whose commit fails StoreError::Unknown after landing leaves reset_changed_rows false
- file: CHANGELOG.md
  line: 98
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the bullet says a reset that meets an unknown commit loses the store, but one met on a world row answers HostError::Store and keeps the store in use
- file: crates/spindle-kernel/src/lib.rs
  line: 187
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: ExecuteError is documented "Nothing was written" while its Store(StoreError::Unknown) case may have written
- file: crates/spindle-telemetry/src/catalogue.rs
  line: 69
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: command.<Command>.failed.store is documented "the command did not execute" and also counts commands whose commit outcome is unknown
```

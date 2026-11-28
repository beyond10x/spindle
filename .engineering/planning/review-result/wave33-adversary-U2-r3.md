---
format: aep.planning-md/3
id: review-result:wave33-adversary-U2-r3
kind: review-result
status: active
title: Adversary, round 3, on wave 33 U2
relations:
- reviews: story:new-leader-write-while-old-paused
revision: 1
---
unit: story:new-leader-write-while-old-paused, wave 33 U2. Findings cover `e1284b080`, which is head `2addec31` plus my one test commit.
verdict: nothing found. No case is red. One CONFIRMED note: the unit's suite let a Retry-After mutant (D3) live, and the new case kills it. One CONFIRMED note about the docs.
cases: executed 215→220, red 0
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 10 paths (part 6)
needs-coordinator: the managed probe worktree `spindle-w33-u2-adv3-mutant` is marked finished, with its cache discarded. `worktree gc` is left to you.

**1. `git --no-pager diff --stat 2addec31..HEAD`**
```
 .../tests/spindle-ingress/adversary_w33_u2.rs      | 388 +++++++++++++++++++++
 1 file changed, 388 insertions(+)
```
Only the test file changed. Rounds 1–2 cases are unchanged. The new cases sit in `mod round_3` (line 577) with their own `Held` stub store.

**2. Cases added** in `<local path>`. All are green now and each runs in under 0.01 s.
- `round_3::an_events_only_command_held_answers_503`: `BookDesk booked` is the `commit_events` path, which no earlier stub held. It answers `503` `{"refused":"store: …","committed":false}`. The request log records it `failed`, and no row is written.
- `round_3::a_refusing_command_on_a_held_store_answers_its_refusal`: a weekend booking answers its declared 4xx refusal `closed`, not 503. Nothing reaches the write.
- `round_3::a_held_command_sent_again_once_the_store_answers_is_written_once`: this checks the docs' "safe to send again". The first send answers 503; the same request sent again answers 202 `opened`. One row is written, and the log reads failed, then served.
- `round_3::the_busy_503_carries_no_retry_after`: checks D3 A (no Retry-After header) and a JSON Content-Type.
- `round_3::a_held_consequence_keeps_the_written_command_s_answer`: uses the fixture `refused-bound`. `till` Post commits, and the bound `balance.Apply` hits the held store. The request answers 2xx `posted`, not 503.

First run of the cases alone (`cargo test --locked -p spindle-ingress --test spindle-ingress adversary_w33_u2::round_3`): 3 ok, 2 FAILED. Both failures were mistakes in my tests, not in the code:
- The request log lists the newest request first.
- The `adversary-unpublished` TouchTicket answers `501 committed:true` "desk does not publish helpdesk.tickets.Noted" before any binding runs.

I fixed both: the cases now sort by `sequence` and use the `refused-bound` fixture. Second run: `test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 194 filtered out; finished in 0.01s`.

Mutant probe, run in a managed worktree at `2addec31`. The mutation was `Answer::busy` with `headers: vec![("retry-after".to_owned(), "1".to_owned())]` (diff in scratch). Run: `cargo test … -- busy_store adversary_w33_u2`. Result: `test result: FAILED. 10 passed; 1 failed; 1 ignored`. Only `round_3::the_busy_503_carries_no_retry_after` failed, at :903. `busy_store` and the round 1–2 cases stayed green.

**3. Suite run** (after the cases existed):
- `cargo test --locked -p spindle-ingress --no-fail-fast`: exit 0. lib `22 passed`; integration `198 passed; 0 failed; 1 ignored … finished in 37.23s`.
- With my cases deselected (`-- --skip adversary_w33_u2::round_3`): `193 passed; 0 failed; 1 ignored; 5 filtered out`. That gives the 215 before.
- `cargo clippy --locked -p spindle-ingress --all-targets -- -D warnings`: exit 0.
- `cargo fmt -p spindle-ingress --check`: exit 0.

**4. Judgement findings** (tree `e1284b080`)
- `crates/spindle-ingress/src/answer.rs:97` (`Answer::busy`), mutant, note, CONFIRMED, introduced. No test of the unit pinned D3 A. A Retry-After added to the busy 503 passed `busy_store` and rounds 1–2. The kept round-3 case now catches it. Nothing needs to change in the code.
- `website/docs/concepts/capabilities.md:243` and the `CHANGELOG.md:84` entry, contract-drift, note, CONFIRMED, introduced.
  - **Measured:** the unit's sentence says "A read held so is not answered `503`" without exception. The run's help text says otherwise (`crates/spindle-cli/src/cli.rs:162-164`, and `Listener::stopping` in `listener.rs`). During a drain after a signal, an events or history read still waiting for the store is cut off and answered `503`.
  - **What reaches it:** a SIGTERM, such as a rolling restart, inside the up-to-10 s window. I wrote no test, because a cancellable stub reader does not reproduce `Reader::stopped_by`.
  - **Fix:** a few words in the sentence: "outside a drain".

**5. Attacked and could not break**
- Commands that create, move, carry only events, or refuse all answer as the docs say. Busy from a read inside a command is still 503. Other store failures are still 501.
- Writes go only to the running component's store, so `Routed`/`Counted`/`Spelled` never split a commit, and `committed:false` holds.
- eventlog 0.10.0 `append_guarded` and `atomic_group` return Deadline only before COMMIT; an error from COMMIT itself becomes UnknownCommit (already filed). `bounded` is not used for any write that commits.
- eventlog-sqlite 0.10.0 never returns `Deadline` (searched every file), and Spindle adds no timeout of its own. "A SQLite store reports no deadline … answers 501" holds.
- Lock wait of 2 s, statement timeout of 5 s, idle-in-transaction of 10 s (eventlog `PoolOptions::default`): these match the docs.
- Other held reads: the view and events/history (round 2), plus metrics 500 and dead-letters 500. None answers 503.
- Under a lease, `serve_with_unless` reads a 503 with `committed:false` as uncommitted (`status >= 500`), so the lease check still runs.
- The reset is parked. Replay answers its own documented 500, or 200 `failed`.
- `state_move` matches only Overloaded's text. Nothing else matches on `StoreError::Backend` text.

**6. Paths written outside the worktree**
- `<local path>` holds these files:
  - `pg-lib.rs` (eventlog source read for reference)
  - `round3-alone.log`
  - `round3-alone-2.log`
  - `mutant-retry-after.diff`
  - `mutant-retry-after.log`
  - `fmt.log`
  - `suite.log`
  - `suite-deselected.log`
  - `clippy.log`
  - `fmtcheck.log`
  - `commit-msg.txt`
- Managed worktree `<local path>`: clean, finished, cache discarded.

**7.**
```findings
- file: crates/spindle-ingress/src/answer.rs
  line: 97
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: no test of the unit pinned D3 A, so a Retry-After header on the busy 503 survived busy_store and rounds 1-2; round_3::the_busy_503_carries_no_retry_after now kills it
- file: website/docs/concepts/capabilities.md
  line: 243
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: '"A read held so is not answered 503" has no exception, but cli.rs:162-164 documents that a held events or history read cut off by a drain answers 503'
```

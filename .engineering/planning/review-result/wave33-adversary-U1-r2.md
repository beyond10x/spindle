---
format: aep.planning-md/3
id: review-result:wave33-adversary-U1-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 33 U1
relations:
- reviews: story:listener-limits
revision: 1
---
unit: story:listener-limits, head 9cf4d69f4 (cases committed on 46ead86a4)
verdict: CONFIRMED
cases: executed 221→226, red 3
origin: introduced 4 / pre-existing 0 / undecided 0
wrote-outside-worktree: 6 files in the scratch directory, plus 1 managed probe worktree (now finished)
needs-coordinator: none

**Summary:** The fix for a client that never reads works, including against a client that reads one byte at a time. Three new cases are red:
- A command whose client left while it was queued is still handed after its request time.
- A connection waiting at the cap is reset when the listener closes, instead of getting a `503`.
- A stalled body holds its place for `BODY_READ` (10 s), not for `Limits::head_read`.

Also, if `Stalling` never reset its timer, the unit's own tests and the round-1 tests would all still pass. A new green case now catches that.

1. Diff, `git --no-pager diff --stat 46ead86a4 HEAD`:
```
 .../tests/spindle-ingress/adversary_w33_u1.rs      | 276 +++++++++++++++++++++
 1 file changed, 276 insertions(+)
```
Only the test file changed. The round-1 cases are kept as they were.

2. Cases added in `<local path>`. Each was run alone first (`cargo test --locked -p spindle-ingress --test spindle-ingress -- --exact adversary_w33_u1::<name>`):

| case | asserts | now | time |
|---|---|---|---|
| `a_command_whose_client_left_is_not_handed_past_its_request_time` | client sends a keep-alive command, then closes. 900 ms later (request time 300 ms), `next` hands nothing | **red** | 0.90 s |
| `a_connection_waiting_at_the_cap_when_the_listener_closes_is_answered_503` | cap 1, one kept-alive connection, a whole command waiting in the backlog. On stopping + `close`, that command gets `503` | **red** | 0.21 s |
| `a_body_that_stalls_frees_its_place_at_the_head_read_time` | cap 1, head read 1 s, a head sent with only part of its body. A second client is answered within 2.5 s | **red** | 3.18 s |
| `a_client_reading_one_byte_at_a_time_does_not_hold_the_cap` | cap 1, answers fill the buffers, then the client reads 1 byte every 250 ms. A second client is answered | green (kept) | 1.27 s |
| `an_answer_read_in_chunks_with_short_pauses_is_written_whole` | a 24 MiB answer, read 3 MiB at a time with 400 ms pauses (head read 1 s), arrives whole and takes more than 2 s | green (kept) | 2.90 s |

Red output when each case was first run:
```
panicked at .../adversary_w33_u1.rs:608:5:
a command queued 900 ms ago, past its request time of 300 ms, its client gone, is handed to the caller's thread: Some("{\"desk_id\":\"left\",\"holder\":\"ada\"}")
test result: FAILED. 0 passed; 1 failed; ... finished in 0.90s
```
```
panicked at .../adversary_w33_u1.rs:651:5:
assertion `left == right` failed: a whole command waiting at the cap when the run stopped taking work is answered: None
  left: None
 right: Some(503)
```
```
panicked at .../adversary_w33_u1.rs:689:5:
a connection stalled in its body held the only place of the cap for 3.071996156s (head read time 1 s): None
```

Mutant check, in my own managed worktree `spindle-w33-u1-adv2-mutant`: I removed `self.stalled = None;` from `Stalling::progress` (`listener.rs:789`) and ran `cargo test --locked -p spindle-ingress --test spindle-ingress -- limits:: adversary_w33_u1::an_answer_read adversary_w33_u1::a_client`:
```
test limits::a_tls_client_that_never_reads_its_answers_does_not_hold_the_cap ... ok
test adversary_w33_u1::a_client_that_never_reads_its_answers_does_not_hold_the_cap ... ok
test adversary_w33_u1::an_answer_read_in_chunks_with_short_pauses_is_written_whole ... FAILED
  ... 14087911 of 25165834 bytes in 1.664863973s
test result: FAILED. 9 passed; 1 failed
```
All 8 `limits::` tests passed on the mutant. That worktree has been reverted and marked finished.

3. Suite, run after the cases existed. `cargo test --locked -p spindle-ingress --no-fail-fast` exited 101 after 18 s:
```
unittests: test result: ok. 23 passed; 0 failed
tests/spindle-ingress/main.rs:
test adversary_w33_u1::a_connection_waiting_at_the_cap_when_the_listener_closes_is_answered_503 ... FAILED
test adversary_w33_u1::a_command_whose_client_left_is_not_handed_past_its_request_time ... FAILED
test adversary_w33_u1::a_body_that_stalls_frees_its_place_at_the_head_read_time ... FAILED
test result: FAILED. 200 passed; 3 failed; ... finished in 11.79s
```
`executed 221` is this run minus my 5 cases (198 + 23); I did not do a separate run with them deselected. `cargo clippy --locked -p spindle-ingress --all-targets -- -D warnings` exited 0, and `cargo fmt -p spindle-ingress --check` exited 0. No new test takes over 5 s.

4. Findings, covering head 9cf4d69f4:

- **F1, client left, command handed late.** `crates/spindle-ingress/src/listener.rs:1152` (`handed`) and `:69` (`Exchange::take`). CONFIRMED, warning, introduced.
  - What happens: the request time is enforced only inside the connection's future. hyper drops that future when the client disconnects, so the exchange stays queued with no deadline, and `next` later hands it to the thread, which commits a command nobody is waiting for. D3 A says `next` skips such a request.
  - Who reaches it: any client or proxy whose own timeout is at or under the request time. `run` hands it in the serve loop (`crates/spindle-cli/src/serve.rs:709`) and during drain.
  - Fix: keep the deadline in the `Exchange`, and have `take` mark it lapsed once that deadline has passed.
- **F2, reset at the cap on close.** `crates/spindle-ingress/src/listener.rs:628` (`drop(listener)`). CONFIRMED, warning, introduced.
  - What happens: on close, the listening socket is dropped while connections still wait in the backlog at the cap. A client that already sent a whole `POST` gets a reset instead of the `503` that `Listener::stopping` and `close` promise, and cannot tell whether the command ran.
  - Who reaches it: SIGTERM on a run whose places are all held (`serve.rs:878-879`).
  - Fix: keep accepting during the grace period and answer `503`, or stop holding places once stopping is set.
- **F3, no-reset mutant survives.** `crates/spindle-ingress/src/listener.rs:789`. CONFIRMED, note, introduced.
  - What happens: no test from the unit or round 1 fails when the reset is removed. `an_answer_read_in_chunks_with_short_pauses_is_written_whole` now covers it.
- **F4, stalled body.** `crates/spindle-ingress/src/surface.rs:1145` (the `Limits::head_read` doc) against `listener.rs:1063` (`BODY_READ`). INFEASIBLE, note, introduced.
  - What happens: the doc says a stalled connection holds its place for at most the head read time, but a stalled body holds it for 10 s.
  - Why only a note: `run` cannot set the head read time, and its default (10 s) equals `BODY_READ`, so only library callers that set a lower head read time see the gap.
  - Fix: bound the body read by the smaller of the two, or correct the doc.

5. Attacked and could not break:
- **One-byte reader:** the kernel opens no window for it, so the write bound still closes the connection.
- **Slow reader of chunks:** it holds a place for as long as it keeps reading, but that costs no more than a normal keep-alive client sending a request every few seconds, so it is not a separate attack.
- **Honest slow reader of a large answer:** gets the whole answer.
- **Client gone after the thread took the command:** read only, not run. The answer fails to send without an error and the command runs once, so nothing is lost or duplicated.
- **503/504 race:** round 1 covered it, and it holds.
- **Drain with a client that never reads:** the shorter of `Stalling` and the close grace ends it.
- **Very large or zero bounds:** `checked_add` handles them, and tokio's sleep has its own handling.
- **spindle-control:** all 5 `same` calls pass their arguments in the right order. Its server has no connection cap, which is outside the story's D5.
- **A read past its time:** it keeps its blocking thread. I could not build a slow read on the in-memory store.

6. Paths written outside the worktree:
- In `<local path>`: `case1.log`, `cases2-5.log`, `mutant.log`, `suite.log`, `clippy.log`, `commit-msg.txt`.
- The managed worktree `<local path>`: clean, cache discarded, finished, waiting for `worktree gc`.

7.
```findings
- file: crates/spindle-ingress/src/listener.rs
  line: 1152
  category: concurrency
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a queued command whose client disconnected is handed by Listener::next after its request time, because the lapse is decided only in the dropped connection future
- file: crates/spindle-ingress/src/listener.rs
  line: 628
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: on close the listener drops its socket while connections wait at the cap, so a whole command in the backlog is reset instead of answered 503
- file: crates/spindle-ingress/src/listener.rs
  line: 789
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: removing the progress reset in Stalling::progress passes every limits and round-1 test; only the new chunked-reader case fails on it
- file: crates/spindle-ingress/src/surface.rs
  line: 1145
  category: contract-drift
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: Limits::head_read says a stalled connection holds its place at most that long, but a stalled body holds it for BODY_READ (10 s); run cannot set a lower head read time
```

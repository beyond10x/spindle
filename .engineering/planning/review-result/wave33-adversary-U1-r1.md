---
format: aep.planning-md/3
id: review-result:wave33-adversary-U1-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 33 U1
relations:
- reviews: story:listener-limits
revision: 1
---
unit: story:listener-limits, head ec79b0baf (adversary cases committed on top as 2de1fd7b9)
verdict: CONFIRMED
cases: executed 216→220, red 1
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 5 paths (part 6)
needs-coordinator: none

1. `git --no-pager diff --stat ec79b0baf HEAD` (both paths are test files):
```
 .../tests/spindle-ingress/adversary_w33_u1.rs      | 418 +++++++++++++++++++++
 .../spindle-ingress/tests/spindle-ingress/main.rs  |   1 +
 2 files changed, 419 insertions(+)
```

**Summary:** A client that sends requests and never reads the answers holds its place in the connection cap with no time limit. Neither the head read time nor the request time ever starts for it. With the default cap of 512, that many such connections stop the listener for every client, health probes included. Slow-loris by head bytes, a stalled TLS handshake at the cap, and the 503/handed race all held.

2. **Cases added** in `<local path>` (the `mod` line is in `main.rs`, sorted):

- **`a_client_that_never_reads_its_answers_does_not_hold_the_cap`: RED.**
  - Setup: cap 1, head read 1 s, request time 1 s. One client pipelines `GET /__spindle/live` until its writes block and never reads.
  - Asserts: a second client is answered within 3 s.
  - Took 4.7 s.
  - Captured when run alone:
    ```
    panicked at crates/spindle-ingress/tests/spindle-ingress/adversary_w33_u1.rs:147:5:
    a client that never reads (7968677 bytes of requests sent, none of the answers read) holds the only place of the cap past the head read time and the request time: a second client got no answer within 3s
    test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 196 filtered out; finished in 4.74s
    ```
- **`a_head_sent_byte_by_byte_frees_its_place_at_the_head_read_time`: green, kept.** Cap 1, head read 1 s, one byte every 50 ms. The client behind it is answered in under 2.5 s, and the slow connection is closed. hyper's head timer starts once per head and is not reset by each byte.
- **`a_stalled_tls_handshake_frees_its_place_at_the_head_read_time`: green, kept.** A TLS listener with cap 1 and head read 1 s, and a partial ClientHello. The TLS client behind it completes its handshake and is answered in under 2.5 s.
- **`no_command_answered_503_is_ever_handed`: green, kept.** Request time 300 ms, 24 concurrent commands, the thread takes one every 40 ms.
  - Each command handed answered 201 or 504.
  - Each one not handed answered 503 with `committed: false`.
  - The two sets add up to 24, both are non-empty, and nothing is handed afterwards.

3. **Suite run**, after the cases existed: `cargo test --locked -p spindle-ingress --no-fail-fast` exited 101.
```
test result: ok. 23 passed; 0 failed; ... finished in 0.01s            (unittests)
test adversary_w33_u1::a_client_that_never_reads_its_answers_does_not_hold_the_cap ... FAILED
test result: FAILED. 196 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 11.18s
```
The "before" count of 216 is this run's 220 minus my 4 cases. I made no separate run with my module deselected.

Gate checks on my file:
- `cargo clippy --locked -p spindle-ingress --all-targets -- -D warnings`: exit 0, after I rewrote one loop as `while let`.
- `rustfmt --check` on the module: clean.
- Module rerun: `3 passed; 1 failed ... finished in 4.73s`.

4. **Findings**, against head ec79b0baf.

- **F1, `crates/spindle-ingress/src/listener.rs:766` (`answering`): NEEDS-CHANGE, blocker, introduced.**
  - **What was measured:** red case `adversary_w33_u1.rs:147`.
  - **Why:** hyper's head timer runs only inside `poll_read_head`. That is not polled while a response is in flight and cannot be flushed (hyper 1.12.0 `dispatch.rs` `poll_read_head` → `dispatch.poll_ready`). No write deadline exists, so the place is held as long as the client keeps the socket open.
  - **What reaches it:** any TCP client of a served `run`, on loopback or off it with `--address`. 512 such clients by default.
  - **Before the change:** with no cap, the same client held only its own connection.
  - **Fix to name:** bound how long an answer may wait to be written, for example close a connection whose write makes no progress within the head read time or the request time.
- **F2, `crates/spindle-ingress/src/listener.rs:332` (also `crates/spindle-control/src/server.rs:175`): INFEASIBLE, note, introduced.**
  - The loop runs the secret's length every time, and the unit's byte counter proves that.
  - But `given.get(index)` takes a different path when the index is inside or past the presented value. In theory the time changes until the presented length passes the secret's length.
  - The byte counter cannot see this, and I found no way to measure it over HTTP.

5. **Attacked and could not break:**
- Slow-loris by head bytes at the cap.
- A stalled TLS handshake at the cap (it uses `Limits::head_read`).
- A command that lapsed while queued being handed afterwards. `next` and the deadline compare-and-swap on one claim, and `close` answers without handing.
- A 503 `committed:false` going to a command that was handed (24-way race).
- The idle keep-alive close.
- `connections: 0` (it is clamped to 1).
- The unit's mutants. Its tests catch removing `max_headers`, removing the LAPSED claim, and `next` ignoring the claim.
- The compare's length leak in its loop count (every length case is in the unit's test).

6. **Paths written outside the worktree**, all under `<local path>`:
- `red1.log`
- `module.log`
- `suite.log`
- `clippy.log`
- `commit.txt`

7. Findings block:
```findings
- file: crates/spindle-ingress/src/listener.rs
  line: 766
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a client that pipelines requests and never reads the answers holds a connection-cap place forever, because hyper's head read timer does not run while an answer cannot be written and no write deadline exists, so 512 such clients stop the listener for everyone
- file: crates/spindle-ingress/src/listener.rs
  line: 332
  category: judgement
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: the length-hiding compare (and its copy at spindle-control server.rs:175) takes a different path for an index inside or past the presented value, a timing difference the byte counter cannot see and that was not measurable here
```

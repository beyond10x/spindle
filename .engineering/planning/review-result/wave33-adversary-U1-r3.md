---
format: aep.planning-md/3
id: review-result:wave33-adversary-U1-r3
kind: review-result
status: active
title: Adversary, round 3, on wave 33 U1
relations:
- reviews: story:listener-limits
revision: 1
---
I found two red cases and broke none of round 2's three fixes. Both reds are places where the documents say something the code does not do; neither is a fault in the fixes themselves.

```
unit: story:listener-limits, head 1324f05bd (971ae8292 + adversary test commit), worktree spindle-w33-listener-limits
verdict: CONFIRMED
cases: executed 226→233, red 2
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 3 paths (part 6)
needs-coordinator: none
```

**1. Diff stat** (`git --no-pager diff --stat 971ae8292..HEAD`)
```
 .../tests/spindle-ingress/adversary_w33_u1.rs      | 384 +++++++++++++++++++++
 1 file changed, 384 insertions(+)
```
It touches one test file and no implementation file. The new cases are appended after round 2's; no earlier case was changed. Committed as 1324f05bd with `b10x-gates bot`. The check for names and home paths printed nothing.

**2. Cases added** (`crates/spindle-ingress/tests/spindle-ingress/adversary_w33_u1.rs`)

First run, the seven cases alone: `cargo test --locked -p spindle-ingress --test spindle-ingress -- --test-threads=1 <7 names>` → `5 passed; 2 failed`, finished in 5.36 s, exit 101. Every case runs under 5 s.

- **Red:** `a_body_arriving_steadily_is_not_refused_as_stalled` (:706). The head read time is 1 s and the body arrives in 16 pieces, 100 ms apart, 1.6 s in all. Output:
  `a body that never paused for more than 100 ms, 1.6 s in all, is refused as stalled at the head read time of 1 s (handed: false): Some((408, "{\"refused\":\"the body did not arrive within 1s\"}"))`
- **Red:** `a_probe_waiting_at_the_cap_when_the_run_stops_is_answered_503` (:814). With a cap of 1, `GET /__spindle/live` waits in the backlog; then the stopping flag is set and `close(2 s)` is called. Output:
  `assertion left == right failed: a probe waiting at the cap when the run stopped: Some((200, "{\"bound_ms\":60000,\"idle_ms\":201,\"live\":true}"))`
- **Green, kept:** `a_client_still_sending_its_body_after_the_408_does_not_hold_the_cap`. A client that keeps sending its body after its 408 still frees its place within 2.5 s.
- **Green, kept:** `a_flood_of_connections_does_not_keep_a_closing_listener_accepting`. 16 threads keep connecting during the close. `close(500 ms)` returns within 2 s, and the socket takes no connection more than 1 s after `close` was called.
- **Green, kept:** `a_request_sent_slowly_from_the_backlog_during_the_close_is_answered`. A request whose head arrives in two parts 500 ms apart during the close is answered 503.
- **Green, kept:** `a_connection_taken_from_the_backlog_at_close_answers_one_request_and_closes`. Of two pipelined requests, only the first is answered (503), then the connection closes; the close takes under 2.5 s.
- **Green, kept:** `no_command_is_both_handed_and_refused_at_a_tight_request_time`. 40 commands, a request time of 30 ms, takes delayed 0 to 9 ms. Each handed command answers 201 or 504, each command not handed answers 503 with `committed:false`, and every command is one of the two.

**3. Suite run** (after the cases existed)

`SLOT_TIMEOUT=30m cargo test --locked -p spindle-ingress --no-fail-fast`, exit 101:
- unit tests: `ok. 23 passed`
- integration tests: `FAILED. 208 passed; 2 failed; ... finished in 21.37s`
- doc tests: 0

The two failures are the two red cases above. `executed before` (226) is the 233 cases run minus my 7; I did not make a separate run with them deselected. `cargo fmt -p spindle-ingress --check` passes, and `cargo clippy --locked -p spindle-ingress --all-targets -- -D warnings` passes.

**4. Findings** (both cover 1324f05bd)

- **F4 doc drift (warning, CONFIRMED, introduced).**
  - What was measured: the body read is one bound on the whole body's arrival, `crates/spindle-ingress/src/listener.rs:1116` (`BODY_READ.min(head_read)` wrapped around the whole collect). The documents call it a stall: `surface.rs:1144` ("how long a request's body may stall"), `CHANGELOG.md:24` ("whose body stalls for 10 s"), and the `run --max-connections` help in `cli.rs` and `cli.md`. So a body that never pauses but takes longer than the bound is refused 408.
  - What reaches it: through `run`, a body that takes over 10 s to arrive, such as a large upload on a slow link, is refused while the help promises that only a stall is. The 10 s bound itself existed at the base; the "stalls" wording is new. Through `Surface::limited` with a head read time under 10 s, the refusal itself is new.
  - Fix: either word it as "whose body takes over N s to arrive", or make the timer reset on each piece of the body that arrives.
- **F2 help drift (note, CONFIRMED, introduced).**
  - What was measured: the `run --max-connections` help (`cli.rs:601`, and `cli.md`) says each connection waiting at the cap when the run stops is "taken and answered `503`". The probes under `/__spindle/` do not follow that: `live` answered 200, and by reading the code `requests` would too.
  - What reaches it: a liveness probe from a load balancer that is waiting at the cap during shutdown.
  - Fix: one doc clause, "requests outside `/__spindle/`". The `--drain` help already has the same gap at the base.

**5. Attacked and not broken**
- The deadline race between `next` and the waiting connection: both sides decide through one atomic compare-and-swap on the claim. No command was both handed and answered 503, and none taken was answered `committed:false` (the 30 ms case above, plus reading the code).
- The backlog drain at close under a flood: it ended, and the socket accepted nothing after it.
- A slow request from the backlog during the drain: answered 503.
- A connection taken from the backlog: not kept alive, one answer, then closed.
- The 408 and the cap: the place is freed even when the client keeps sending its body.
- `cli.md` matches `cli.rs` for the three flags.

**6. Paths written outside the worktree**
- `<local path>`
- `<local path>`
- `<local path>`

**7. Findings block**
```findings
- file: crates/spindle-ingress/src/listener.rs
  line: 1116
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the body read bounds the whole body's arrival, yet Limits::head_read (surface.rs:1144), CHANGELOG.md:24 and the run --max-connections help say only a body that stalls is refused 408, so a steadily arriving body over the bound is refused (adversary_w33_u1.rs:706)
- file: crates/spindle-cli/src/cli.rs
  line: 601
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: run --max-connections says each connection waiting at the cap when the run stops is answered 503, but a probe under /__spindle/ taken from the backlog is answered 200 (adversary_w33_u1.rs:814)
```

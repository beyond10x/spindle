---
format: aep.planning-md/3
id: review-result:wave33-adversary-U1-r4
kind: review-result
status: active
title: Adversary, round 4, on wave 33 U1
relations:
- reviews: story:listener-limits
revision: 1
---
```
unit: story:listener-limits, attacked at head 43850e41f; findings cover c6544a4a4 (head plus my round-4 cases)
verdict: CONFIRMED (no red case; one mutant gap, now closed by a kept case; two note-level findings)
cases: executed 232→239, red 0
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 13 paths (part 6)
needs-coordinator: none
```

Nothing I tried breaks `paced`. A pause bound doubled (`pause * 2`) passed every test the unit and rounds 1-3 wrote. My new round-4 case is the only test that catches it.

**1. Diff stat** (`git diff --stat 43850e41f..HEAD`, committed as c6544a4a4 on `unit/w33-listener-limits`):
```
 .../tests/spindle-ingress/adversary_w33_u1.rs      | 263 +++++++++++++++++++++
 1 file changed, 263 insertions(+)
```
It touches test files only. The earlier cases and the two coordinator comments are unchanged.

**2. Cases added**, all in `<local path>` after `// Round 4`. Each was run alone first. All are green now:

| Case | What it asserts | Alone |
|---|---|---|
| `a_body_that_pauses_past_the_head_read_time_is_refused_408_from_its_last_piece` | head_read 1 s; six pieces 300 ms apart, then the client stops; answer is `408`, 0.8 to 1.6 s after the last piece | PASS 2.8 s |
| `a_body_that_ends_before_its_announced_length_is_refused_400` | Content-Length 100, 11 bytes sent, client shuts its write side; answer is `400` | PASS 0.01 s |
| `an_empty_body_is_handed_at_a_short_head_read_time` | head_read 200 ms; `Content-Length: 0` and no length header; both are handed and answered `201` | PASS 0.05 s |
| `a_body_of_exactly_the_limit_arriving_steadily_is_handed` | exactly `BODY_LIMIT` bytes in 16 pieces 100 ms apart, head_read 1 s; handed, `201` | PASS 1.6 s |
| `tiny_chunks_sent_after_the_411_do_not_hold_the_cap` | cap 1; chunked body refused `411`; client keeps sending 1-byte chunks; a second client gets `200` within 2.5 s; the chunked connection is closed | PASS 0.07 s |
| `at_a_head_read_time_over_ten_seconds_a_paused_body_is_refused_at_ten_seconds` | cap 1, head_read 12 s, body paused; `408` 9.5 to 11.5 s after the head; the place is free afterwards | PASS 10.0 s (over 5 s) |

I wrote two more cases and removed them before committing. They sent `Content-Length: N, N`, expecting the listener to read the body and answer `400` where its docs promise `413`. The over-limit one was red with `Some((400, ""))`. The control case showed this is hyper refusing the header itself: `Content-Length: 34, 34` also got `400` with an empty body and was never handed (`(handed: false): Some((400, ""))`). So there was no finding, and I did not keep a case that only pins hyper's behaviour.

Mutant probe, run in a separate copy (managed worktree `spindle-w33-u1-adv4-mutant` at c6544a4a4); the unit tree was not touched:
- **Mutant 1, a pause answered `400` instead of `408`:** caught by `grants::the_transport_limits_stay_before_the_grant_check` and by my pause case (`237 passed, 2 failed`).
- **Mutant 2, `timeout(pause * 2, body.frame())`:** only my pause case catches it (`238 passed, 1 failed`). Its red output:
  ```
  panicked at .../adversary_w33_u1.rs:1144:5:
  the 408 came 2.001373307s after the last piece (head read time 1 s)
  ```

**3. Suite run**, after the cases existed: `cargo nextest run --locked -p spindle-ingress --no-fail-fast` → `Summary [37.722s] 239 tests run: 239 passed, 0 skipped`, exit 0, 53 s wall time. The "before" count of 232 is 239 minus my 7 cases; I did not run the suite separately without them. `cargo fmt -p spindle-ingress --check` and `cargo clippy --locked -p spindle-ingress --all-targets -- -D warnings` are both clean.

**4. Findings that are not failing cases** (they cover c6544a4a4):
- **`listener.rs:1187`, mutant, CONFIRMED, introduced.** Before round 4, nothing checked the size of the pause bound. With it doubled, a stalled body holds its place for twice head_read, and the round-2 stall case (2.5 s limit) stays green. The kept pause case now catches it, so nothing needs to change in the code. What reaches it: any `Limits::head_read` set by a library caller. `run` always uses 10 s.
- **`listener.rs:394`, contract drift, CONFIRMED, introduced.** The `Listener::bind` docs still list only "one that does not arrive within `BODY_READ` `408`". They leave out the pause bound this unit added, which refuses a body that arrives within 10 s but pauses longer than head_read. Fix: one doc line naming the pause bound. The `Limits::head_read`, `BODY_READ`, CLI help, CHANGELOG and `AGENTS.md` texts all match the code.
- **`listener.rs:1129`, judgement, CONFIRMED, introduced.** At head_read 10 s, the only value `run` uses, the pause bound and the 10 s bound expire together. Which refusal text the client gets ("paused for longer than 10s" or "did not arrive within 10s") depends on which timer fires first. Under mutant 1 the `grants` case got the pause branch at 10 s, so the pause text is what a client meets. The status is `408` either way.

**5. Attacked and not broken:**
- An empty body: no pause bound runs.
- A declared length that ends early: answered `400`.
- Chunked bodies: refused `411` before `paced` runs, and the cap is released.
- A body over the limit: a plain length is refused `413` before reading; `N, N` is refused `400` by hyper before the listener sees it.
- Exactly the limit, arriving steadily: handed.
- head_read of 10 s or more: `408` at 10 s, and the place is freed.
- The place in the cap is released after a pause `408`, a 10 s `408` and a `411`.

**6. Paths written outside the worktree:**
- `<local path>` with 12 files: `case1.log`, `commit.txt`, `suite.log`, `mutant-m1.log`, `mutant-m2.log` and 7 per-case logs.
- `<local path>`, the managed mutant copy. It is restored to clean and marked finished, but `worktree gc` keeps it because c6544a4a4 is not on any remote yet.

My leases on both trees are released.

```findings
- file: crates/spindle-ingress/src/listener.rs
  line: 1187
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a pause bound doubled (timeout(pause * 2, ...)) passed the whole suite through round 3; only the round-4 case a_body_that_pauses_past_the_head_read_time_is_refused_408_from_its_last_piece catches it
- file: crates/spindle-ingress/src/listener.rs
  line: 394
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: Listener::bind still names only the BODY_READ 408 and leaves out the pause bound by Limits::head_read that this unit added
- file: crates/spindle-ingress/src/listener.rs
  line: 1129
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: at the head read time run always uses (10 s) the pause and whole-body bounds expire together, so a stalled body's 408 text depends on which timer fires first
```

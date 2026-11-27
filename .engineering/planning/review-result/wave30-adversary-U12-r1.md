---
format: aep.planning-md/3
id: review-result:wave30-adversary-U12-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 30 U12
relations:
- reviews: story:served-clock-steps
revision: 1
---
```
unit: story:served-clock-steps (wave 30 U12), implementor head 4d8ea7be, findings cover 68cbcd8f (4d8ea7be + adversary tests)
verdict: CONFIRMED
cases: executed 644→650, red 2
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 7 paths (part 6)
needs-coordinator: none
```

I found two defects: one warning and one note. Both are red tests now committed on `unit/w30-clock`. The other four new tests pass and stay as checks.

**1. Proof of what I touched** (`git diff --stat 4d8ea7be..HEAD`). Both are test files, so no implementation file changed:
```
 .../tests/spindle-cli/adversary_w30_u12.rs         | 316 +++++
 crates/spindle-cli/tests/spindle-cli/main.rs       |   1 +
```
Commit `68cbcd8f`, made through `b10x-gates bot -- commit`. The check for names and home paths printed nothing.

**2. Cases added**, all in `<local path>`:

- **Red: `a_step_to_the_instant_get_answered_moves_nothing_and_is_answered`.**
  - What it does: steps the clock to `2026-07-01T00:00:00.0005Z`, reads `GET /api/clock`, then posts back the instant that GET answered.
  - What it expects: `200`, because the code's own doc says a step to where the clock stands moves nothing.
  - What happens: the clock keeps nanoseconds, but GET and the step's answer spell it only to the millisecond, so the post-back counts as a step back. The refusal names the very instant it refuses. Output from running this case alone:
    ```
    panicked at crates/spindle-cli/tests/spindle-cli/adversary_w30_u12.rs:212:5:
    assertion `left == right` failed: a step to the instant GET /api/clock answered is a step to where the clock stands
      left: (409, Object {"refused": String("the clock stands at 2026-07-01T00:00:00Z; it moves forward only")})
     right: (200, Object {"at": String("2026-07-01T00:00:00Z")})
    ```
- **Red: `an_event_before_1970_carries_the_clock_instant`.**
  - What it does: starts a run with `--clock 1969-07-20T20:17:40Z`. `GET /api/clock` answers that instant correctly. Then it books once.
  - What happens: the event's `at` reads `1970-01-01T00:00:00Z`. The cause is `micros()` in `host.rs` (around line 2636), which returns 0 for any time before 1970, so the command's sequence is `max(0, last+1)`. Output from running this case alone:
    ```
    panicked at crates/spindle-cli/tests/spindle-cli/adversary_w30_u12.rs:229:5:
    assertion `left == right` failed: the event carries the instant its command ran: [Object {"at": String("1970-01-01T00:00:00Z"), ... "payload": Object {"booking_id": String("b1")}}]
      left: String("1970-01-01T00:00:00Z")
     right: "1969-07-20T20:17:40Z"
    ```
- **Green: `a_step_and_a_world_at_the_clock_instant_are_answered`.** A step and a reset world at exactly the current instant each answer `200`. The implementor's tests never step or reset to an equal instant, so changing `at < stands` to `<=` in `reset_to`, or refusing an equal instant in `step_clock`, would pass them. This test fails on either change. I reasoned this from the code; I did not build a mutated copy.
- **Green: `a_malformed_step_is_refused_400_and_moves_nothing`.** Ten bad bodies all answer `400` and the clock stays put. They include a leap second (`23:59:60Z`), a date only, no offset, 10 fraction digits, a number, an extra key, a wrong key, an array, bare text and an empty body. Nothing tested `400` before, so answering `409` instead would have passed.
- **Green: `a_step_reads_an_offset_and_a_fraction`.** `+02:00` and `-04:00` offsets resolve to the right UTC instant, `.250Z` comes back as `.25Z`, and the next event's `at` matches the clock.
- **Green: `a_refused_world_leaves_the_clock`.** A reset with a later `clock` but two rows for the same booking answers `400`, empties nothing and leaves the clock where it was. This fails if the clock step is moved ahead of the row check.

**3. Suite runs**, after the cases existed:
- `cargo test --locked -p spindle-cli --no-fail-fast`: exit 101, `test result: FAILED. 640 passed; 5 failed; 33 ignored; ... finished in 31.31s`, plus 5 unit tests passed. The 5 failures are my 2 red cases and the 3 UI tests that need `ui/dist`, which your brief already lists as failing.
- Same command with `-- --skip adversary_w30_u12`: exit 101, `636 passed; 3 failed; 33 ignored; ... 6 filtered out; finished in 36.46s`, plus 5 unit tests. That run gives the "before" count of 644.
- `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings`: clean. `cargo fmt -p spindle-cli --check`: exit 0.
- No test I added is slow: the 6-case module finished in 0.16 s.

**4. Findings**, covering 68cbcd8f:
- **F1, warning, CONFIRMED, introduced.**
  - What was measured: the red output above (`adversary_w30_u12.rs:212`).
  - Where: `crates/spindle-host/src/host.rs` `rfc3339`, which floors to the millisecond through `view::instant`, while `step_clock` compares at full precision.
  - What reaches it: any client that sends a timestamp finer than a millisecond to `POST /api/clock`, a reset world's `clock` or `--clock`, and then reuses what the run answered.
  - Fix: either refuse or truncate sub-millisecond instants when stepping, or answer at full precision.
- **F2, note, CONFIRMED, introduced.**
  - What was measured: the red output above (`adversary_w30_u12.rs:229`).
  - Where: `crates/spindle-host/src/host.rs` `micros` and `civil`. The `at` field on `/api/events` is new in this unit. `/api/history` already gave the same wrong time for pre-1970 clocks on the base.
  - What reaches it: `--clock` with any instant before 1970. I found no user of that, which is why it is only a note.
  - Fix: make sequence-derived times correct before 1970, or refuse such a `--clock`.

**5. What I attacked and could not break:**
- `409` on both methods without `--clock`, and on a reset world carrying `clock`, with nothing emptied.
- `409` for a step back with the clock unchanged, and `400` for an earlier world clock with nothing emptied.
- The proxy secret on both `GET` and `POST` (`403`).
- A specification route at `/api/clock` refused at start. The check is by path, so any method or a templated route is caught.
- `405` with `Allow: GET, POST`.
- Clock routes answered on the host thread, never alongside a running command.
- The embedded and router paths: `spindle-embed` goes through `spindle_ingress::route`, and every `reset_to` caller goes through `console::reset`.
- The guard and generated timestamps read the stepped clock, because `stepping` swaps `self.clock`.
- Row checks (`at_rest`) never read the clock, so the order of writing rows and moving the clock has no visible effect.
- Leap-second text and year 9999.
- One risk I did not test: `Host::stepping` on a host opened with a different clock would leave earlier readers and periodic timers on the old clock. `serve.rs` hands over the same clock, so nothing reaches it today.

**6. Paths written outside the worktree**, all under `<local path>`:
- the directory itself
- `red1.log`
- `red2.log`
- `module.log`
- `suite.log`
- `suite-before.log`
- `commit-msg.txt`

The pool also built artifacts under the worktree's `target/` on the build node.

**7. Findings block**

```findings
- file: crates/spindle-host/src/host.rs
  line: 2888
  category: boundary
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: GET /api/clock and the step answer floor the clock to the millisecond while step_clock compares nanoseconds, so a step to the instant the run itself answered is refused 409 naming that same instant (red at crates/spindle-cli/tests/spindle-cli/adversary_w30_u12.rs:212)
- file: crates/spindle-host/src/host.rs
  line: 2636
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: on a --clock before 1970 every /api/events record's new at reads 1970-01-01T00:00:00Z because micros() is 0 before the epoch, not the clock instant the command ran at (red at crates/spindle-cli/tests/spindle-cli/adversary_w30_u12.rs:229)
```

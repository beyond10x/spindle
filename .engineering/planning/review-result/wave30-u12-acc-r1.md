---
format: aep.planning-md/3
id: review-result:wave30-u12-acc-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on wave 30 (U12)
relations:
- reviews: story:served-clock-steps
revision: 1
---
needs-revision

story:served-clock-steps — the acceptance line "`a_published_event_carries_its_command_time` (`spindle-host` `console.rs`)" names a test and no expected value, so any test with that name passes it; it does not say that one command on a host opened on `ManualClock::at(T)` gives `published()[0].at == T` — .engineering/planning/story/served-clock-steps.md:84

story:served-clock-steps — "each record of `GET /api/events` ... names the clock instant its command ran" does not step the clock between two commands, so an `at` taken from the `--clock` start instant, or from the clock when the events are read, passes just like the command's own time — .engineering/planning/story/served-clock-steps.md:77-78

story:served-clock-steps — the Outcome says "without `--clock` the route answers `409`" (line 55) for both methods, but the acceptance checks only `POST /api/clock`, so a `GET /api/clock` that answers the system time on a run without `--clock` passes — .engineering/planning/story/served-clock-steps.md:74

story:served-clock-steps — neither the Outcome nor the acceptance says how a reset world that carries `clock` is answered on a run without `--clock` (accepted, ignored, `400` or `409`), so no answer an implementor picks for it can be checked — .engineering/planning/story/served-clock-steps.md:56-57,75-76

What I read: 1 story (story:served-clock-steps, whole body, via `aep plan artifact show` and the store file) and the wave page's U12 rows and lines. I also checked the story lifecycle and the tree. The `clock-served` fixture sets the deadline at 2026-06-01 under `deadline > now`, so `early` at 2026-02-01 and `late` at 2026-07-01 hold. A console route without the proxy secret answers `Answer::not_proven()`, which is 403 (`answer.rs:63-69`, `listener.rs:667`). `router.rs`, the host's `console.rs`, `ManualClock::at` and the setup at `adversary_trusted_caller.rs:75` all exist.

What I could not establish:
- Not checked: whether an event's `at` reads the same as the stepped instant to the millisecond after several commands at one instant. The Scope's own note says it can be n−1 µs past (host.rs:2380-2382). I did not check what `civil` shows.
- Out of my lane (parallel-safety): the wave page lists U12 on `cli.rs` and the generated `reference/cli.md` (wave-30.md:92-93), and the next bullet still says "U9 only" for the same files (wave-30.md:94-95).
- Out of my lane (parallel-safety): wave 29 also changes `host.rs`. It is named only in general terms (wave-30.md:101), and U12's hunks in `Published`, `merged`, `World` and `reset_to` are not checked against wave 29's escalating-bindings hunks.
- Public-repository check: no person, organisation, host or home path appears in the story body.

```findings
- file: .engineering/planning/story/served-clock-steps.md
  line: 84
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the acceptance line \"`a_published_event_carries_its_command_time` (`spindle-host` `console.rs`)\" names a test and no expected value, so any test with that name passes it; it does not say that one command on a host opened on `ManualClock::at(T)` gives `published()[0].at == T`"
- file: .engineering/planning/story/served-clock-steps.md
  line: 77
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "\"each record of `GET /api/events` ... names the clock instant its command ran\" does not step the clock between two commands, so an `at` taken from the `--clock` start instant, or from the clock when the events are read, passes just like the command's own time"
- file: .engineering/planning/story/served-clock-steps.md
  line: 74
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome says \"without `--clock` the route answers `409`\" (line 55) for both methods, but the acceptance checks only `POST /api/clock`, so a `GET /api/clock` that answers the system time on a run without `--clock` passes"
- file: .engineering/planning/story/served-clock-steps.md
  line: 56
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "neither the Outcome nor the acceptance says how a reset world that carries `clock` is answered on a run without `--clock` (accepted, ignored, `400` or `409`), so no answer an implementor picks for it can be checked"
```

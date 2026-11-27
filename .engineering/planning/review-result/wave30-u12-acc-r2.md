---
format: aep.planning-md/3
id: review-result:wave30-u12-acc-r2
kind: review-result
status: active
title: Plan critic (acceptance), round 2, on wave 30 (U12)
relations:
- reviews: story:served-clock-steps
revision: 1
---
needs-revision

story:served-clock-steps — the acceptance says `POST /api/clock` to 2026-07-01 "answers `{"at"}`" but does not say what value `at` must have, so an answer that carries the old instant or the wall-clock time still passes; it should say the answer is `{"at": "2026-07-01T00:00:00Z"}` — .engineering/planning/story/served-clock-steps.md:90

All four round-1 findings (`review-result:wave30-u12-acc-r1`) are fixed:
- **Line 84, now 107-108:** the test now names its value: "events whose `at` is `T` to the millisecond".
- **Lines 77-78, now 98-101:** the clock now steps between two commands, and each command's `at` is named.
- **Line 74, now 93-95:** both `POST` and `GET /api/clock` answer `409` without `--clock`.
- **Lines 56-57 and 75-76, now 71-74 and 93-95:** a reset world carrying `clock` without `--clock` answers `409` and empties nothing. The embedded host has its own case at 109-110.

What I read: 1 story, `story:served-clock-steps`, the whole body, with `aep plan artifact show` and the store file. I also read `review-result:wave30-u12-acc-r1`, the diff of `8f5049d4..HEAD` for the story and the wave page, and the wave page's U12 lines (33-34, 55, 85-100, 106-111). In the tree I checked who calls `read_world`, the console routes in `spindle-embed/src/lib.rs:175`, and the `400` answers of `console.rs` `reset` (538, 549).

What I could not establish:
- I did not raise this one. The Outcome (lines 72-74) and D1 A say an embedded host and a Node instance answer `409`. The acceptance checks only a reset on the embedded host, not `GET`/`POST /api/clock` there, and nothing on a Node instance. They all reach the same `read_world` and the host's own answer, so I took the embedded case as standing for them.
- Today a reset world carrying `clock` already gets `400`, because `Body` refuses unknown fields and `console.rs:538,549` answer `400`. So the "earlier `clock` answers `400`" half of `a_reset_world_moves_the_clock` passes before the work. It is still a real change because the forward half fails today.
- Public repository: no person, organisation, host or home path appears in the story body.
- Out of my lane: none seen.

```findings
- file: .engineering/planning/story/served-clock-steps.md
  line: 90
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the acceptance says `POST /api/clock` to 2026-07-01 \"answers `{\"at\"}`\" but does not say what value `at` must have, so an answer that carries the old instant or the wall-clock time still passes; it should say the answer is `{\"at\": \"2026-07-01T00:00:00Z\"}`"
```

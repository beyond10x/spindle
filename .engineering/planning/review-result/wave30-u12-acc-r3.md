---
format: aep.planning-md/3
id: review-result:wave30-u12-acc-r3
kind: review-result
status: active
title: Plan critic (acceptance), round 3, on wave 30 (U12)
relations:
- reviews: story:served-clock-steps
revision: 1
---
approve

What I read: the one story I was given, `story:served-clock-steps` (whole body, revision 17), using `aep plan artifact show story:served-clock-steps`. I also read `review-result:wave30-u12-acc-r1` and `-r2`, the story diffs in HEAD (`0d7d1880`) and HEAD~1 (`b8e29ffc`), and the U12 lines of `.engineering/waves/wave-30.md` (33-34, 55, 85-95, 100, 106-112). I checked `crates/spindle-embed` in the tree: `src/lib.rs` and `Opening` at :95-108.

Both earlier rounds are resolved:
- **Round 2, line 90:** fixed. The acceptance now reads "`POST /api/clock` to 2026-07-01 answers `{"at": "2026-07-01T00:00:00Z"}`" (story file, around line 92).
- **Round 1:** all four findings are still fixed after this revision: the expected value at 107-108, the stepped clock between two commands, `409` for both methods, and `409` for a reset world carrying `clock` without `--clock`.
- **Each acceptance line checks a change:** each fails before the work and passes after it. Today `/api/clock` does not exist. A reset world carrying `clock` gets `400` because `Body` is `deny_unknown_fields`, where the acceptance expects `409`. Today's `/api/events` records have no `at`. `crates/spindle-embed/tests` does not exist yet, which matches "first and only integration-test binary".

What I could not establish:
- **Embedded test setup:** the embedded `Opening` has `worlds: bool`. Without it, a reset carrying a world "is refused `400`" (`crates/spindle-embed/src/lib.rs:104`). So `an_embedded_host_refuses_a_reset_world_carrying_clock` can only get `409` on an opening with `worlds: true`, or if the clock check runs before the worlds check. The acceptance leaves this choice to the implementor. Either way the test fails before the work, so I did not raise it.
- **Embedded clock:** `Opening` also has `clock: Arc<dyn Clock>` (`lib.rs:107`), so an embedded host could be handed a manual clock. Whether it should still answer `409` is a design question, outside my lane.
- **Stale Scope line:** the Scope's Tests bullet still says "`POST /api/clock` 2026-07-01 answers `{"at"}`" (story § Scope, Tests). The Acceptance overrides it, so I did not raise it.
- **Node instance:** the Outcome says a Node instance also answers `409`, but no test checks it. As in round 2, I took the embedded case as covering it, because both go through `read_world`.
- **Public repository:** no person, organisation, host or home path in the story body (grep of the story file).
- **Out of my lane:** none seen.

```findings
[]
```

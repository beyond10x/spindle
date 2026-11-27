---
format: aep.planning-md/3
id: review-result:wave28-par-r3
kind: review-result
status: active
title: Plan critic (parallel-safety), round 3, on wave 28
relations:
- reviews: story:console-routes-off
- reviews: story:structured-json-logs
- reviews: story:metrics-series-bounded
- reviews: story:seed-readiness
- reviews: story:health-and-graceful-drain
- reviews: story:entity-event-history
- reviews: story:events-record-their-caller
- reviews: story:environment-binds-external-decisions
- reviews: story:replay-with-decisions
- reviews: story:one-time-response-admission
revision: 1
---
approve

All 10 stories are placed and their shared surfaces are named on the wave page, so no pair collides unannounced.

Round-2 findings, both resolved:
- **`--decisions` help in `cli.rs:259-265` (U6 and U7):** `.engineering/waves/wave-28.md` now gives both units this hunk, states "U7 merges before U6", and says "U6 rewrites the `--decisions` help over U7's text".
- **`tests/spindle-cli/seed.rs` (U4 and U5):** U5 now puts its test in a new file, `tests/spindle-cli/seed/caller.rs`. The page names its one `mod` line at the end of `seed.rs` against U4's `mod` edit at `:1225-1230`, and orders "U5 merges before U4". U5's story Scope (`events-record-their-caller.md:602`) matches.

Round-1 findings (`router.rs`, `observe.md`, `environment-and-admission.md`) still hold as resolved; the page keeps their hunks and merge orders.

New checks this round, all clean:
- **`families.json` and `ess/domains/observe.yaml`:** `families.json` is a hand-written fixture validated by `Taskfile.yml:304`. U3's new family needs no edit to `observe.yaml`, so the page's "U3 only" for `families.json` and "U4 only" for `observe.yaml` hold.
- **U5's `Store::outcome` and `Origin` changes:** the `Store` implementors and `Origin` literals in the tests of `spindle-host`, `spindle-ingress` and `spindle-kernel` sit in files no other unit lists. Compile fallout stays inside U5.
- **Remaining overlaps:** the `main.rs` and `serve.rs` hunks (U1 to U4, U6, U7) and the test `main.rs` `mod` lines are named on the page with their units.

What I read: the 10 stories (`aep plan artifact show` on each), `aep plan artifact graph`, `.engineering/waves/wave-28.md`, `review-result:wave28-par-r1` and `-r2`, and the tree at `plan/wave-28` (`Taskfile.yml`, `ess/domains/observe.yaml`, `ui/src/api/types.test.ts`, and a `git grep` for `Store` implementors and `Origin`/`Committed` literals). Surfaces: 10 stories placed with at least one cited path, 0 placed only by inference (inferred paths remain as secondary entries), 0 unplaceable. Acceptance reads traced to a producer in the set: 4. Two have a `depends_on` edge: `story:entity-event-history` on `story:events-record-their-caller`, and `story:health-and-graceful-drain` on `story:seed-readiness`. The other two have no edge but the page orders them: U2's "every stderr line" after U1, U3 and U4, and U1's `Console::ALL` iteration after U5. Both of these checks pass in any merge order.

What I could not establish:
- Whether the `Serving` struct literal in `main.rs` (about `:604`) and `serve.rs:72` merge cleanly across U1, U3 and U4. The page names the files, units and merge order, but "separate hunks" may not hold for adjacent field additions, so the coordinator may meet a textual conflict.
- Whether U1's doc edit of `with_console` and U5's `Console::ALL` use in `surface.rs` merge cleanly. Both are named; I did not flag it.
- Whether U6's rewrite of the `--decisions` help keeps U7's `--scenario` sentence. This is a merge-time content risk, not a surface collision.
- Out of my lane, not counted: the two U7 stories share `Replays::run` with no `depends_on` edge. The page and the Scope of `story:one-time-response-admission` state the order, and the stories run in one unit.

```findings
[]
```

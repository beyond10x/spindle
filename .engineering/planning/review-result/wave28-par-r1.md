---
format: aep.planning-md/3
id: review-result:wave28-par-r1
kind: review-result
status: active
title: Plan critic (parallel-safety), round 1, on wave 28
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
needs-revision

- story:seed-readiness — the wave page's Surfaces section says `crates/spindle-ingress/src/router.rs` is "U5 only", but this story lists it too (`routing` 123-140, a caller of `observe::reserved`; surface **cited** by the tree, 4 `Reserved` sites at `router.rs:123,126,342,429`; inferred as an edit site), so U4 and U5 both land there and the wave page and both bodies say otherwise; either record U4/U5 sharing `router.rs` (as an edge or a stated merge hunk split), or narrow the Scope so U4 does not touch it — .engineering/waves/wave-28.md:58, .engineering/planning/story/seed-readiness.md:119, .engineering/planning/story/entity-event-history.md:111
- story:environment-binds-external-decisions — it adds "a new section after `:153`" of `website/docs/concepts/environment-and-admission.md` (**inferred**), and `story:seed-readiness` edits that file's § A seed submitted at start, lines 153-240 (**inferred**), which is the same region; the wave page's Surfaces section lists neither this file nor U4 and U6 as sharing it; state the shared file and region, by an ordering edge or by moving one section — .engineering/planning/story/environment-binds-external-decisions.md:80, .engineering/planning/story/seed-readiness.md:126
- story:metrics-series-bounded — it rewrites `website/docs/concepts/observe.md:47-64` (**cited**), and `story:seed-readiness` and `story:health-and-graceful-drain` (U4) both list `website/docs/concepts/observe.md` (**inferred**); the wave page's Surfaces section omits this file, so U3 and U4 read as independent; name the shared file, by an ordering edge or by splitting the doc surface — .engineering/planning/story/metrics-series-bounded.md:105, .engineering/planning/story/health-and-graceful-drain.md:120, .engineering/planning/story/seed-readiness.md:126

What I read: 10 stories (`aep plan artifact show` for each, `aep plan artifact graph`) and `.engineering/waves/wave-28.md`. I also grepped the tree at `plan/wave-28` (`router.rs`, `listener.rs`, `surface.rs`, `console.rs`, `Store` implementors, `Environment` literals). Surfaces: 10 cited, 0 inferred only, 0 unplaced. Acceptance reads traced to a producer in the set: 2 (`story:entity-event-history` on `story:events-record-their-caller`, `story:health-and-graceful-drain` on `story:seed-readiness`); both have a `depends_on` edge, 2 of 2. `story:seed-readiness` also reads the reset reseed, produced by `story:reset-reapplies-seed`, which is outside the set and `implemented`.

What I could not establish:
- Whether U4's new `Reserved` variants force match arms in `router.rs`. That is why the `router.rs` claim rests on the story's inferred line, with the `Reserved` sites in the tree as support.
- Whether the adjacent hunks in `surface.rs` (U1 doc text of `with_console` at about 1505-1511, U5's `Console::ALL` use inside the same function) merge cleanly. The wave page names both and I did not flag it.
- Out of my lane, not counted: `story:console-routes-off` Acceptance lists four routes, while `story:entity-event-history` adds a fifth to `Console::ALL` on the premise that the U1 switch covers it. No acceptance in either story pins that, and no `depends_on` edge runs between them.
- The two stories in U7 share `Replays::run` with no `depends_on` edge. The wave page and the `story:one-time-response-admission` Scope both state the order, so I did not flag it.

```findings
- file: .engineering/planning/story/seed-readiness.md
  line: 119
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the wave page says crates/spindle-ingress/src/router.rs is U5 only, but this story's Scope also lists it (routing 123-140, a caller of observe::reserved; the tree has Reserved sites at router.rs:123,126,342,429), so U4 and U5 collide on it unnamed; the edit site for U4 is inferred; record the shared file as an ordering edge or split the surface"
- file: .engineering/planning/story/environment-binds-external-decisions.md
  line: 80
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "adds a new section after line 153 of website/docs/concepts/environment-and-admission.md, the region story:seed-readiness edits (section A seed submitted at start, 153-240); both surfaces are inferred, and the wave page's Surfaces section names neither the file nor U4 and U6 sharing it; state the collision as an ordering edge or split the surface"
- file: .engineering/planning/story/metrics-series-bounded.md
  line: 105
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "edits website/docs/concepts/observe.md:47-64 (cited), which story:seed-readiness and story:health-and-graceful-drain (U4) also list (inferred); the wave page's Surfaces section omits the file, so U3 and U4 read as independent; name the shared file as an ordering edge or split the doc surface"
```

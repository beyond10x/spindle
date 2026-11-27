---
format: aep.planning-md/3
id: review-result:wave30-u12-des-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on wave 30 (U12)
relations:
- reviews: story:served-clock-steps
revision: 1
---
approve

**What I read:** 6 artifacts. I read U12 `story:served-clock-steps` in full, plus `story:events-cursor-across-restart` (U7) and `story:rally-example-reads-stdin` (U9) in full, and `story:node-export-clock-and-seeds`, which sits outside the set. I grepped the other ten stories' scopes for the files U12 touches, and read the wave page (units table, Surfaces, merge order). Commands run: `aep plan artifact show`, `relations`, `graph`, `validate` and `waves`, and `git grep` on `wave/29`.

**What I found:**
- **Edges:** I walked 11 edges, some outside the set: U12 to `vision:spindle`, U12 to `story:clock-and-mint-in-every-mode` (implemented) and on to `story:run-fails-on-rejected-line`, U12 to U7, U7 to `story:entity-event-history`, U9's one edge, and the four edges of `story:node-export-clock-and-seeds`.
- **Cycles and chains:** there is no cycle. The only `depends_on` edge for U12 is U12 to U7, and U7 needs nothing first, so U12 adds no chain.
- **Acceptance reads:** U12 has 8 acceptance lines, and I traced what each one reads.
  - Only one read depends on another story in the set: the `after=` page of `/api/events` in `every_event_record_carries_at`, whose cursor U7 rewrites. The U12 to U7 `depends_on` edge already records this.
  - Everything else already exists in the tree: the `clock-served` fixture in `clock_and_mint.rs`, the trusted-caller setup, and the test modules `router.rs` (spindle-ingress) and `console.rs` (spindle-host).
- **Split and slice:** no other story builds part of the stepping clock, and U12 reaches through host, ingress and cli in one story, so it is not cut by layer.
- **Wave 29:** `wave/29` is still at the wave 28 merge and adds no `Published {` constructor. Wave 30 forks after wave 29 merges, so U12's work on `Published` and `merged` lands on top of wave 29's changes to `host.rs`.

**What I could not establish:**
- **Shared file not listed (parallel-safety lane):** U6 `story:decimal-identity-any-spelling` also changes `crates/spindle-host/src/world.rs` (`seed`, :115), and U12 changes `Body` and `read_world` in the same file. The wave page's Surfaces section does not list `world.rs`. The merge order already puts U12 after U6.
- **Missing case (acceptance/scope lane):** every world reader goes through `spindle_host::read_world`: the served reset, `spindle-embed/src/lib.rs:213` and `spindle-node`. U12's Outcome does not say what a reset world carrying `clock` gets on a host with no stepping clock: a run without `--clock`, an embedded host, or a Node instance.
- **Unproven:** the guard on the new route is inherited from `listener.rs:667` without new code. The story marks this "Step 2, unproven", and I did not test it.

```findings
[]
```

---
format: aep.planning-md/3
id: review-result:wave30-u12-par-r2
kind: review-result
status: active
title: Plan critic (parallel-safety), round 2, on wave 30 (U12)
relations:
- reviews: story:served-clock-steps
revision: 1
---
Both round-1 findings look addressed. Next I'm checking whether U12's new surfaces (spindle-embed, the four host test files, the status pages) collide with other units.
approve

**Round-1 findings (both resolved):**
- The `world.rs` collision with U6 is now stated in two places. The story's "Would collide with" line (`.engineering/planning/story/served-clock-steps.md:142`) names it, and the wave page's Surfaces section (`.engineering/waves/wave-30.md:90-91`) gives U6 `seed` and U12 `Body` / `read_world`, with U12 after U6.
- The two contradicting `cli.rs` / `cli.md` bullets are fixed. `main.rs` alone is now U9's, and the other line points to the shared bullet above it (`.engineering/waves/wave-30.md:94-96`).

**U12 reviewed again:**
- **New surfaces have no partner in the set.** U12 now also changes `crates/spindle-embed/src/lib.rs` and a new `crates/spindle-embed/tests` (both inferred), four `spindle-host` test files that build a `World` (cited), and the generated `website/docs/status.md` and `website/data/status.json` (cited). No other wave-30 story lists any of these. U2 mentions `spindle-embed/src/lib.rs:231` only as a reference and does not change it.
- **U6's world test does not break when `World` gains `clock`.** U6 adds its test to `adversary_world_integers.rs`, which loads a world through `spindle_host::read_world` (line 262) rather than writing the struct out by hand.
- **Merge order still matches.** U12 merges last, after U7 (the `depends_on` edge is at graph line 2425), after U6 for `world.rs`, and after U9 for `cli.rs`. Wave 29 does not change `world.rs` or `spindle-embed`, and wave 30 starts after wave 29 merges.

**What I read:** U12 and the wave page in full, the round-1 parallel-safety record, the diff of the last two commits of `plan/wave-30`, and the scope lists of the other 13 stories. Commands:
- `aep plan artifact show` on the round-1 record and `aep plan artifact graph`
- `git grep 'World {'`
- `grep` and `awk` over the story files
- `sed` on `crates/spindle-docs/src/status.rs`
- `ls` on `crates/spindle-embed`

**Counts:**
- Stories placed by citation: 14. Placed only by inference: 0. Unplaced: 0. U12's two `spindle-embed` paths are its only inferred surfaces.
- Acceptance reads traced to another story in the set: 1. U12's `every_event_record_carries_at` on an `after=` page reads U7's events cursor, and a `depends_on` edge records it. The new embedded-host test reads only U12's own `read_world` change.

**What I could not establish:**
- Where U12's stepping handle sits in `host.rs` is still inferred. If it goes beside `Host::seeded`, it touches U6's `establish`. The story's "Would collide with" line now says this, so it is not a finding.
- Out of my lane (acceptance): the test list under Scope (`served-clock-steps.md:136`) still says `a_served_run_without_clock_refuses_to_step` answers only 409. The Acceptance section (`:93-95`) expects 409 for both methods and for a reset whose world carries `clock`.
- Out of my lane (design/scope): `crates/spindle-embed` has no `tests` directory yet. U12's embedded-host test creates the crate's first integration-test binary, which must follow the one-binary-per-crate rule.

```findings
[]
```

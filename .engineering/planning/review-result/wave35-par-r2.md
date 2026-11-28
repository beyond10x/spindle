---
format: aep.planning-md/3
id: review-result:wave35-par-r2
kind: review-result
status: active
title: Plan critic (parallel-safety), round 2, on wave 35
relations:
- reviews: story:link-bound-setting
- reviews: story:observe-label-vocabulary
- reviews: story:unknown-commit-in-seed-and-retries
revision: 1
---
approve

Both round-1 findings are resolved, and I found nothing the revision (b9f8e424) broke.

**Round-1 finding 1 (reader beside its producer): resolved.**
- `aep plan artifact graph` now shows `story:unknown-commit-in-seed-and-retries -> story:observe-label-vocabulary [depends_on]`.
- U3's body names the edge, the shared generated files, and the instruction to regenerate with `task generate:observe` on top of U2's merged change (`.engineering/planning/story/unknown-commit-in-seed-and-retries.md`, "Wave 35 order").
- The wave page's old "no depends_on inside the wave" line is gone. It now says there are two edges inside the wave, added at round 1 (`.engineering/waves/wave-35.md:65-67`).

**Round-1 finding 2 (`cli.rs` and `cli.md`): resolved by admitting the shared file.**
- U3 now cites `crates/spindle-cli/src/cli.rs` and `website/docs/reference/cli.md` in its scope.
- U3 states it edits the `--seed` help of `run` (`:536-585`) and U1 edits `Command::Play` (`:695-751`). These are different variants, so a merge keeps both sides.
- A second edge, `story:unknown-commit-in-seed-and-retries -> story:link-bound-setting [depends_on]`, records U3 regenerating `cli.md` on top of U1.
- The wave page's Surfaces section lists both shared files and the U3-after-U1 order, and the U3 row in the units table includes both.

**What the revision might have broken (I found nothing):**
- The pairs I checked are U1 and U2, U1 and U3, and U2 and U3.
  - U1 and U2 share only `CHANGELOG.md`, which the wave page names.
  - U1 and U3 share `cli.rs`, `cli.md` and `CHANGELOG.md`, which both U3's body and the wave page name.
  - U2 and U3 share `observe.yaml` (different types), the generated files and `CHANGELOG.md`, which both name.
- No stale claim remains that U1 shares only `CHANGELOG.md`.
- U1's and U2's own bodies do not name the new U3 edges. U3's body and the wave page do, and the edge is recorded in the store. I do not count that as a finding, because the one body that has to change is U3's.
- The "Would collide with" line in U3's Scope was updated to say U3 depends on both stories and merges after them.
- The new `--seed` help text and its `cli.md` row add no test. The wave still adds none to the PostgreSQL lane.

**What I read:** 3 stories, the wave page and `review-result:wave35-par-r1`. I ran `aep plan artifact show` on each, `aep plan artifact graph` and `aep plan artifact relations`, `git show --stat` on b9f8e424, and `grep` over the stories and wave page for `depends_on`, `cli.rs`, `cli.md` and U3.

**Placement:** I placed all 3 stories (cited surfaces mixed with a few inferred). I could not place none.

**Acceptance reads traced to a producer:** 2 (U3 reads U2's regenerated files, and U3 reads U1's regenerated `cli.md`). Both are recorded by a `depends_on` edge.

**What I could not establish:** none within my lane. Test budgets, no network, no Python, PostgreSQL-lane margin and public-repo hygiene are outside my lane, and I did not review them.

```findings
[]
```

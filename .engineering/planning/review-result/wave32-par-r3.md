---
format: aep.planning-md/3
id: review-result:wave32-par-r3
kind: review-result
status: active
title: Plan critic (parallel-safety), round 3, on wave 32
relations:
- reviews: story:postgres-lane-runs-locally
- reviews: story:conform-awaits-system-steps
- reviews: story:leader-exits-promptly-on-sigterm
- reviews: story:seed-under-a-lease
revision: 1
---
needs-revision

- `story:leader-exits-promptly-on-sigterm` — its 20 evidence runs read the local PostgreSQL lane that `story:postgres-lane-runs-locally` creates, and no `depends_on` edge runs from it to that story. The wave page orders U3 after U1 only in prose, so `aep plan artifact waves` could schedule U3 beside U1 and U2. The story's own acceptance does not say it needs U1. Either add `depends_on story:postgres-lane-runs-locally`, or say the 20 runs fall back to a hosted cluster. — `.engineering/planning/story/leader-exits-promptly-on-sigterm.md:69-70` (reader), `.engineering/waves/wave-32.md:73-74` (the prose claim), `.engineering/planning/story/postgres-lane-runs-locally.md:26-32` (producer)
- `.engineering/waves/wave-32.md` — `CHANGELOG.md` `[Unreleased]` is edited by all four units (cited for U4, inferred for U1, U2 and U3), yet the page names it only as a U1/U2 share (`:63`) and a U3/U2 share (`:74`). `[Unreleased]` is empty today, so every unit inserts at the same spot. U4's merge after U3 and the U1 and U3 pairs are not named. Name every pair, or make `[Unreleased]` entries a merge-time step, or give each unit its own subsection. This is cited. — `.engineering/waves/wave-32.md:63,74`, `CHANGELOG.md:7`, `.engineering/planning/story/seed-under-a-lease.md:14-15`

Pairs I checked:
- **U3 and U4 collide as the page says.** They share `serve.rs`, `database.rs`, kernel `postgres.rs` `Lease`, `cli.rs` (plus the generated `cli.md`) and `postgres/replicas.rs`. The `depends_on` edge is on U4's story (`seed-under-a-lease.md:9`), and the page lists the hunks at `:75-83`.
- **U1/U2, U1/U3 and U2/U3 are disjoint** on source files. U2's tree is `spindle-conformance`, `replay.rs` and one design-doc row; U1's is `Taskfile.yml`, `ci.yml`, `scripts/` and `AGENTS.md`. U3 touches none of these. `.config/nextest.toml` is outside every unit.
- **The integration branch overlaps U4 only**, on `seed.rs` `Seeding::end` and `adversary_w31_u6.rs:904-912`. I read both hunks in its tree, and they match the page's description (a 3-hunk `seed.rs` diff, 3 new overrides, a widened priority-10 filter and 12 test files). The page orders it before the fork. The line numbers U4 cites are positions on `main`, which shift by a few lines after the integration lands.
- **U1 and U4 share `AGENTS.md` without sharing a line.** U4 edits line 40, a one-line crate-table row. U1 edits lines 132-136.

1. What I read: four stories, the wave page, `CHANGELOG.md`, `Taskfile.yml:180-197`, `ci.yml:185-214`, `AGENTS.md:40,128-138`, `cli.rs:156-166,545-580`, `.config/nextest.toml`, the `Reading` implementations, and the integration tree's `git status`, `git diff` and `git merge-base`. No `aep plan artifact` commands were run.
2. What I could not establish:
   - **Surface counts:** 4 items placed by cited surface, 0 by inferred surface only, 0 unplaced (several individual files within them are inferred).
   - **Reads traced to a producer:** 3 (U3 to U1's lane, U4 to U3's exit path, U4 to U1's lane). One is recorded by an edge: U4 on U3.
   - **U3's conditional surface:** it lists `store.rs`, `rows.rs`, `sqlite.rs` and kernel `postgres.rs` as inferred and only if D1 A changes the `Reading` signature. `Reading` has other implementations (`crates/spindle-host/tests/spindle-host/events_range.rs:166`, `crates/spindle-ingress/tests/spindle-ingress/concurrent_reads.rs:100`, `store.rs:1458`, `sqlite.rs:2610`) that no story lists. This is conditional, so it is not a finding.
   - **Integration branch state:** it is uncommitted, so the page's description of it is true only of the working tree. The untracked `.pglane.docker.sh` sits in that tree and must not be committed with it (the page's "must not" list binds units, not that branch).
   - **Out of my lane:**
     - `wave-32.md:86` says U1 "rewrites" the `TESTS` filter, but U1's story only drops `requires` and moves the body into an inner task.
     - U2's three tests each synthesize scenarios from a fixture, which the test-budget rule may treat as per-test cost.
     - U3's D1 scope is wide and conditional.

```findings
- file: .engineering/planning/story/leader-exits-promptly-on-sigterm.md
  line: 69
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its 20 evidence runs read the local PostgreSQL lane story:postgres-lane-runs-locally creates (wave-32.md:73-74), and no depends_on edge runs from this story to that one, so a wave scheduler could hold both at once; add the edge or state the hosted-cluster fallback (cited)"
- file: .engineering/waves/wave-32.md
  line: 74
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "CHANGELOG.md [Unreleased] (empty, CHANGELOG.md:7) is edited by all four units but the page names only the U1/U2 and U3/U2 shares (:63, :74); the U1/U3 pair and U4's share with U1, U2 and U3 are unnamed (cited)"
```
needs-revision

Both round-1 findings are fixed. I found one new gap.

Round-1 findings:
- **U3 reading U1's lane:** `story:leader-exits-promptly-on-sigterm` now has `depends_on: story:postgres-lane-runs-locally` (`leader-exits-promptly-on-sigterm.md:9`). The page records both edges (`wave-32.md:69`), and the 20 evidence runs name the lane U1 starts (`leader-exits-promptly-on-sigterm.md:79-81`).
- **`CHANGELOG.md` `[Unreleased]`:** the page now names it as shared by all four units, with a bullet per story id and merges that keep both sides (`wave-32.md:83-85`).
- **Closing-line item on the `TESTS` filter:** `wave-32.md:114-117` now says U1 moves the filter into an inner task and does not rewrite it.

New finding:
- `.engineering/waves/wave-32.md` — the `max_connections` headroom is measured once, on U1's tree (`postgres-lane-runs-locally.md:70-73`, a sampled peak below the script's value). U1 merges first, and U3 and U4 then add one and nine process tests that run under `~replicas::` in the same 12-thread postgres profile. Neither unit nor the page re-samples the peak (`wave-32.md:97-102` asks only for the lane's time). `check-postgres-options.sh` checks the script's configured value only. A 300-connection ceiling cleared at U1's test count can be exceeded after U4. In wave 31 the lane failed on "too many clients" in 2 of 3 runs. Have U3 and U4 each re-sample the peak and name it in their report beside the lane time, or have the page say the peak is checked once and accepted. The surfaces are cited.

New surfaces I checked and found clear:
- **U2's new `crates/spindle-cli/tests/spindle-cli/replay.rs`:** the file exists, `mod replay` is already in `main.rs:94`, and no other unit lists it.
- **U1's new `scripts/postgres-lane.test.sh` and `check:postgres-lane-script`:** only U1 edits `Taskfile.yml` outside the existing `check:postgres` block, and no other unit touches `scripts/`.
- **U4's kernel test file `crates/spindle-kernel/tests/spindle-kernel/postgres.rs`:** U3 also adds tests there. The U3→U4 edge covers it, and the page names the file at `wave-32.md:114-117`.
- **The integration branch:** the surfaces it shares with U4 are unchanged, and its state is still uncommitted.

1. What I read: the `git diff` from `c5c207b4` to `b7f11c5a` over the four stories and the wave page, the `ls` of the CLI test directory with its `mod` lines, and the `grep` of the other stories for `replay`. Same tree as round 1, with the round-1 reads carried over.
2. What I could not establish: 4 items placed by cited surface, 0 by inferred surface only, 0 unplaced. Reads traced to a producer: 3 (U3 to U1's lane, U4 to U3's exit path, U4 to U1's lane); all three now have an edge (U3 on U1, U4 on U3). The U4-to-U1 read is covered transitively through U3. The integration branch is still uncommitted, so the page describes its working tree only. The untracked `.pglane.docker.sh` in that tree must not be committed. Out of my lane: the new acceptance numbers in U3 (7 s bound, 1.33 s standby takeover).

```findings
- file: .engineering/waves/wave-32.md
  line: 97
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "story:postgres-lane-runs-locally samples the pg_stat_activity peak once on its own tree before U3 and U4 add their process tests to the same 12-thread postgres profile; neither unit nor the page re-samples the peak or names it in a report, so the 300-connection ceiling checked at U1's test count is not checked at the wave's final count (cited)"
```
approve

The round-2 finding is fixed. At `.engineering/waves/wave-32.md:101-105` U3 and U4 each sample the `pg_stat_activity` peak of one lane run after their tests, name it beside the lane time, and raise `max_connections` in the script and in CI if the peak comes within 50 of it.

The raise falls on U1's files (`scripts/postgres-lane.sh`, `ci.yml`). It cannot collide, because U3 depends on U1 and U4 on U3, so those edits come in order. U3's edit to the end-to-end test (the cursor save ordered before SIGTERM, a hold to 8 s after it) stays inside U3's surface. Nothing new overlaps U2, and U3 still shares no file with U1 or U2. The round-1 and round-2 edges and the `CHANGELOG.md` pair list still hold.

1. What I read: `git diff b7f11c5a b72e8ec87` over the wave page and the four stories, with the round-1 and round-2 reads carried over. The only changed story is `story:leader-exits-promptly-on-sigterm`, and the other three are untouched.
2. What I could not establish: 4 items placed by cited surface, 0 by inferred surface only, 0 unplaced. Reads traced to a producer: 3, all now recorded by an edge. The integration branch is still uncommitted, so the page describes its working tree only. The untracked `.pglane.docker.sh` in that tree must not be committed. Out of my lane: the numbers in U3's test (7 s bound, 8 s hold, 5 s cap on S minus T0).

```findings
[]
```

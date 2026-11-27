---
format: aep.planning-md/3
id: review-result:wave30-u12-par-r1
kind: review-result
status: active
title: Plan critic (parallel-safety), round 1, on wave 30 (U12)
relations:
- reviews: story:served-clock-steps
revision: 1
---
needs-revision

story:served-clock-steps — this story and story:decimal-identity-any-spelling (U6) both change `crates/spindle-host/src/world.rs`, and neither this story's "Would collide with" line nor the wave page's Surfaces section says so (cited in both bodies: U12 changes `Body` at :34-42 and `read_world` at :65-91, U6 changes `seed` at :115). Add the file to Surfaces with both hunks, or split the surface so the two no longer share it. The merge order already puts U6 before U12. — .engineering/planning/story/served-clock-steps.md:116, .engineering/planning/story/decimal-identity-any-spelling.md:80, .engineering/waves/wave-30.md:85-99

.engineering/waves/wave-30.md — Surfaces says two different things about `crates/spindle-cli/src/cli.rs` and the generated `website/docs/reference/cli.md`. One bullet gives them to U12 and U9; the next still says "U9 only". That second bullet should name only `main.rs` as U9's alone. — .engineering/waves/wave-30.md:92-95

What I read: 14 stories. I read U12 and the wave page in full, and the Scope sections of U6, U7 and U9. Commands: `aep plan artifact show` on all 14 stories, `aep plan artifact graph`, the wave-29 page through `git show plan/wave-29:…`, and `git grep` and `sed` on `host.rs`, `world.rs`, `range.rs` and `status.rs`.
- Surfaces: 14 placed by citation, 0 placed only by inference, 0 unplaced.
- Acceptance reads traced to a producer in the set: 1 (U12's `every_event_record_carries_at` on an `after=` page reads U7's events cursor). A `depends_on` edge records it (graph line 2417).
- Other shared files are already named in Surfaces and the merge order: `range.rs` (U7, with an edge), `host.rs` (U1, U6, U7, on separate hunks), `cli.rs`/`cli.md` (U9), `README.md` (U3), `CHANGELOG.md`.
- The `clock-served` fixture exists.
- Wave 29: U12's `host.rs` and `cli.rs` overlap wave 29's U1. That is sequential, because wave 30 forks after wave 29 merges, and the story names it.

What I could not establish:
- Where U12's stepping handle goes. "Beside `clock` :771, opened like `Host::seeded`" suggests a builder after `seeded` (:1235-1268), right above U6's `establish` (:1270-1281). If so, the two hunks touch. This is inferred, so it is not a finding.
- Out of my lane (scope): adding a `pub clock` field to `World` breaks the `World { rows, answers, mint }` struct literals in `crates/spindle-host/tests/spindle-host/concurrent_reads.rs:132`, `rows.rs:246`, `adversary_rows.rs:276` and `adversary_concurrent_reads.rs:78`. None of these files is in U12's scope. No other wave-30 unit touches them, so there is no collision.
- Out of my lane (scope): U12 regenerates `website/docs/status.md` and `website/data/status.json` through `status.rs`, but its scope does not list them. No other wave-30 unit touches them.

```findings
- file: .engineering/planning/story/served-clock-steps.md
  line: 116
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "story:served-clock-steps and story:decimal-identity-any-spelling (U6) both change crates/spindle-host/src/world.rs (cited in both bodies: U12 Body :34-42 and read_world :65-91, U6 seed :115), and neither this story's Would collide with line nor the wave page's Surfaces names it; add the file to Surfaces with both hunks, or split the surface so the two no longer share it (merge order already puts U6 before U12)"
- file: .engineering/waves/wave-30.md
  line: 94
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "Surfaces says two different things about crates/spindle-cli/src/cli.rs and the generated website/docs/reference/cli.md: one bullet gives them to U12 and U9, the next still says U9 only; that second bullet should name only main.rs as U9's alone"
```

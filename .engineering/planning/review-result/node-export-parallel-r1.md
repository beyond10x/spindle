---
format: aep.planning-md/3
id: review-result:node-export-parallel-r1
kind: review-result
status: active
title: Plan critic (parallel), round 1, on the Node export stories
relations:
- reviews: story:node-export-serves-a-specification
- reviews: story:node-export-fixed-actor
- reviews: story:node-export-clock-and-seeds
- reviews: story:node-export-events
- reviews: story:node-export-release-artifact
revision: 1
---
needs-revision

- story:node-export-fixed-actor — it and story:node-export-clock-and-seeds both land on `crates/spindle-embed/` and `crates/spindle-node/` (both inferred, the bodies' own Scope sections) and no depends_on edge or sentence orders them, while both extend the one `Spindle.create(config)` key set that "unknown keys are refused". Remedies: an ordering edge recording the shared files, or split the surface so neither edits the shared config reader and the `lib.rs` surface. — .engineering/planning/story/node-export-fixed-actor.md:35, .engineering/planning/story/node-export-clock-and-seeds.md:38
- story:node-export-clock-and-seeds — it and story:node-export-events both land on `crates/spindle-node/` (inferred, the bodies' own Scope sections), and the clock story adds the host-driving path that the events story's "after the request that published it" delivery hooks into. Neither body mentions the other. Remedies: an ordering edge recording the shared file, or split the surface. — .engineering/planning/story/node-export-clock-and-seeds.md:38, .engineering/planning/story/node-export-events.md:35
- story:node-export-events — it and story:node-export-fixed-actor both land on `crates/spindle-node/` (inferred, the bodies' own Scope sections), and both add tests to `crates/spindle-node/tests/node/*.test.mjs` and `tests/fixtures`. Neither says so. Remedies: an ordering edge recording the shared file, or split the surface. — .engineering/planning/story/node-export-events.md:35, .engineering/planning/story/node-export-fixed-actor.md:35
- story:node-export-serves-a-specification — its scope names `Cargo.lock` (cited, Scope section) and its new crates `spindle-embed` and `spindle-node` add workspace members (`members = ["crates/*"]` in Cargo.toml) plus the `wasm-bindgen` dependency. The parallel branch upgrading the ESS pins edits Cargo.lock and adapts spec/kernel/conformance code, and the body does not say so. The story also calls `spindle-host`, `spindle-ingress` and `spindle-kernel` through `spindle-embed`, so the branch's adaptations to those crates can break its build (inferred). Remedies: record an ordering against that branch (land after it, or rebase onto it before the lane is first run), or split so the new crates land without touching shared lock state. — .engineering/planning/story/node-export-serves-a-specification.md:152-153

**What you read:** 5 artifacts plus epic parent context. Commands: `aep plan artifact show` on all five and on story:service-worker-serves-a-specification, story:periodic-bindings-fire and story:wasm-test-library; `aep plan artifact waves`, which lists 4 of the 5 as `unassessed` and prints `node-export-serves-a-specification` colliding with many pre-existing stories on Taskfile.yml, serve.rs, surface.rs and similar; `git grep`, `ls` and the Cargo.toml `members` line.

Surfaces: 1 item cited by crate path in its body (release-artifact: release.yml and AGENTS.md, marked inferred), 4 inferred (serves and the three follow-ups, the last three only from their body Scope sections), 0 unplaceable. I traced 6 acceptance reads to a producer in the set (the lane `check:node_serves_a_specification` or the `create(config)` surface, read by fixed-actor, clock-and-seeds, events and release-artifact, plus the crate `spindle-node` read by release-artifact and the shared config by fixed-actor/clock). A depends_on edge records all 6.

**What I could not establish:**
- The four follow-ups carry no `scope` metadata in the store, so `aep plan artifact waves` marks them unassessed and cannot see the collisions above. Whether to record `scope` is yours to decide.
- release-artifact's `release.yml` and `AGENTS.md` surfaces are inferred, and `AGENTS.md` is also in the serves scope. An edge already orders those two.
- I did not read the other branch's diff, so which crates it adapts beyond Cargo.lock is inferred from your note.
- Many pre-existing collisions of serves with other stories (for example `surface.rs`, `serve.rs`, `Taskfile.yml`) appear in `waves`. They involve items outside the set and I do not judge them.

```findings
- file: .engineering/planning/story/node-export-fixed-actor.md
  line: 35
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "this story and story:node-export-clock-and-seeds both land on crates/spindle-embed/ and crates/spindle-node/ (inferred, both Scope sections) and both extend the create(config) key set, with no depends_on edge or sentence between them; remedies: an ordering edge recording the shared files, or split the surface"
- file: .engineering/planning/story/node-export-clock-and-seeds.md
  line: 38
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "this story and story:node-export-events both land on crates/spindle-node/ (inferred, both Scope sections) and neither body mentions the other; remedies: an ordering edge recording the shared file, or split the surface"
- file: .engineering/planning/story/node-export-events.md
  line: 35
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "this story and story:node-export-fixed-actor both land on crates/spindle-node/ (inferred, both Scope sections), including src/lib.rs and the node tests and fixtures, and neither says so; remedies: an ordering edge recording the shared file, or split the surface"
- file: .engineering/planning/story/node-export-serves-a-specification.md
  line: 152
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its scope names Cargo.lock (cited) and adds workspace crates, while the parallel branch upgrading the ESS pins edits Cargo.lock and adapts spec/kernel/conformance code the new spindle-embed calls through spindle-host/ingress (the code overlap is inferred); the body does not name that branch; remedies: an ordering against that branch, or a split so the new crates do not touch shared lock state"
```

---
format: aep.planning-md/3
id: review-result:node-export-parallel-r2
kind: review-result
status: active
title: Plan critic (parallel), round 2, on the Node export stories
relations:
- reviews: story:node-export-serves-a-specification
- reviews: story:node-export-fixed-actor
- reviews: story:node-export-clock-and-seeds
- reviews: story:node-export-events
- reviews: story:node-export-release-artifact
revision: 1
---
approve

I read the five stories again. Round 1's findings 1–3 are fixed, and finding 4 is partly fixed and no longer needs a change.

- **Findings 1–3:** The store now has a `depends_on` chain `serves → fixed-actor → clock-and-seeds → events`. Each story also carries `scope` metadata. `aep plan artifact waves` now places the set in waves 34 (serves), 35 (fixed-actor, release-artifact), 36 (clock-and-seeds) and 37 (events). fixed-actor and release-artifact share wave 35, and their scopes are disjoint: `crates/spindle-embed` and `crates/spindle-node` against `.github/workflows/release.yml` and `AGENTS.md`.
- **Finding 4:** The serves body now has an `Ordering:` paragraph. It names the ESS-pin branch, says the story changes no pin, limits its `Cargo.lock` edit to the two new crates' entries, and says it rebases onto that branch and re-runs its lanes before landing.
  - **Branch overlap:** The uncommitted work in the `spindle-ess-099` worktree (`chore/ess-0.99`) also edits `crates/spindle-ingress/src/surface.rs`, `crates/spindle-ingress/src/answer.rs` and `CHANGELOG.md`. The serves scope lists `surface.rs` and `CHANGELOG.md` too, though the paragraph names only the `Cargo.*` files and spec/kernel/conformance code. The rebase remedy covers these files as well, so I record no finding.

**What you read:** 5 stories, the parent epic, and `review-result:node-export-parallel-r1`. Commands: `aep plan artifact show` on each, `aep plan artifact waves`, and `git grep` and `ls` in the worktree and the `spindle-ess-099` worktree.

- **Surfaces:** All 5 stories cite their paths in their bodies (5 cited, 0 inferred, 0 unplaceable). The store's `scope` entries mark them all `inferred`.
- **Acceptance reads:** I traced 6 to a producer in the set, and a `depends_on` edge records all 6.
  - fixed-actor reads the lane that serves creates.
  - clock-and-seeds reads the `create` config from serves and fixed-actor.
  - events reads `GET /api/events` from serves and the host-driving path from clock-and-seeds.
  - release-artifact reads the lane and `SPINDLE_NODE_MODULE` from serves.

**What I could not establish:**
- **Unlisted file:** Acceptance item 4 of serves adds a test to `crates/spindle-cli/tests/spindle-cli/outcome_scoped_refusal.rs`, but that file is not in serves's `scope`. No draft story claims it, since its only claimant, `story:outcome-scoped-refusal`, is implemented, so I do not count it as a collision. Adding it to `scope` would let `waves` see future ones.
- **Unlisted surfaces (inferred):** The acceptances for fixed-actor ("`run`'s reason") and clock-and-seeds ("any randomness the host uses is seeded too") might need code in `spindle-cli`, `spindle-ingress` or `spindle-host`. Their scopes list only `spindle-embed` and `spindle-node`. Both stories sit behind serves in the chain, so any overlap with it is ordered. I found no sign in the tree that either needs those crates.
- **Out of the set:** `waves` prints many collisions between serves and stories outside the set, on `Taskfile.yml`, `surface.rs`, `serve.rs`, `CHANGELOG.md` and `website/docs`. I did not judge them.
- **Out of my lane:** I did not check whether the acceptance lines are verifiable or whether the split is right.

```findings
[]
```

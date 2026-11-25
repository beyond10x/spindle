---
format: aep.planning-md/3
id: review-result:node-export-design-r2
kind: review-result
status: active
title: Plan critic (design), round 2, on the Node export stories
relations:
- reviews: story:node-export-serves-a-specification
- reviews: story:node-export-fixed-actor
- reviews: story:node-export-clock-and-seeds
- reviews: story:node-export-events
- reviews: story:node-export-release-artifact
revision: 1
---
approve

What I read: 7 artifacts (the epic and 5 stories in the set, `review-result:node-export-design-r1`, plus `story:wasm-test-library`, `story:host-opening-is-transport-free` and `story:service-worker-host-matches-run` for context). Commands: `aep plan artifact show` on each, `relations`, `graph` and `validate`, then greps of the tree for the cited fixtures, tests and Taskfile lanes.

Round 1 findings, both closed:
- **Overlap with `story:wasm-test-library`:** the epic's Boundary section and the first story's Outcome now say that story stays as the TypeScript library taking capabilities from TypeScript. This set serves a specification over its routes and takes no capability from JavaScript. No `supersedes` edge is needed.
- **Release story's read of the lane:** the first story's acceptance now defines `SPINDLE_NODE_MODULE` and the default `target/spindle-node`. The lane and `Taskfile.yml` are in the first story's scope. The release story's second acceptance reads that input, and a `depends_on` edge records it.

Round 2 checks:
- **Cycles:** none. I walked 18 edges inside the set and 6 outside it: `story:service-worker-serves-a-specification`, `story:wasm-test-library`, `story:host-opening-is-transport-free`, `story:service-worker-host-matches-run`, `epic:node-export`, `vision:spindle`. The two outside stories that name the first story (`informed_by` and `depends_on`) point one way only.
- **Chain:** fixed-actor, clock-and-seeds and events are ordered one after another. The release story hangs off the first story. Each ordering edge has its reason written beside it in the body (shared `create` configuration and `crates/spindle-node/src/lib.rs`; delivery hooking into the host-driving path). The rubric does not count an ordering edge with a stated shared-file reason as a serialising chain.
- **Acceptance reads:** I traced 12 reads of state a sibling in the set produces. All 12 are covered by a `depends_on` edge. They are the Node lane, `tests/node/*.test.mjs`, `SPINDLE_NODE_MODULE`, the world reader and `mint` seed, `Surface::report`, `Host::advance` and the configuration keys.
- **Split abstraction:** none found. The first story is cut vertically: the `spindle-embed` extraction leaves the Service Worker lane green, and the Node binding is demonstrated by its own lane.
- **Validate:** it printed no finding for the set. The unrelated "without a scope" warnings are not mine.
- **Code cited by the bodies:** `crates/spindle-cli/tests/fixtures/outcome-scoped-refusal` and `unservable`, the review fixture with `exchanges.json` and `decisions.toml`, `tests/spindle-wasm/listener.rs`, and the world reader's `mint` field all exist.

What I could not establish:
- Out of my lane (scope): `story:node-export-fixed-actor` promises `run`'s refusal reasons, which live in `crates/spindle-cli/src/serve.rs`. Its scope names only `spindle-embed` and `spindle-node`. This is the same point I raised in round 1.
- Out of my lane (acceptance): `crates/spindle-node` is described as wasm32-only, yet it holds native recording tests in `tests/spindle-node/`. Whether `cargo` can run those natively is for the acceptance critic.
- Out of my lane (scope): `story:node-export-events` has `spindle-node` only in scope but reads `spindle_host::Reader`. Whether `spindle-embed` needs a change is for the scope critic.

```findings
[]
```

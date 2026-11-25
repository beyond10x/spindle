---
format: aep.planning-md/3
id: review-result:node-export-design-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on the Node export stories
relations:
- reviews: story:node-export-serves-a-specification
- reviews: story:node-export-fixed-actor
- reviews: story:node-export-clock-and-seeds
- reviews: story:node-export-events
- reviews: story:node-export-release-artifact
revision: 1
---
needs-revision

story:node-export-serves-a-specification — its outcome (an in-process wasm host a test starts as an isolated fake backend, with external answers and a world) and the three follow-ups (clock, seeds, events) restate `story:wasm-test-library`'s outcome, yet the body only records `informed_by` and never says what that story keeps or whether it is replaced. Both could end up marked done for one intent; the body should state the boundary, or a `supersedes` edge from here to `story:wasm-test-library` should say it — .engineering/planning/story/node-export-serves-a-specification.md:11, .engineering/planning/story/wasm-test-library.md:20

story:node-export-release-artifact — its second acceptance reads the first story's Node lane test "when pointed at the extracted archive", but neither body says the lane takes the module's path as an input, and this story's scope names no `Taskfile.yml` or `crates/spindle-node/tests/node` to add it. Half of one seam sits in each body. Put the module-path input in the first story's lane (its acceptance 1 loads "from its bytes" with no source named), or put those files in this story's scope — .engineering/planning/story/node-export-release-artifact.md:28, .engineering/planning/story/node-export-release-artifact.md:35

What I read: 6 artifacts (the 5 in the set, plus `story:wasm-test-library`, `story:service-worker-serves-a-specification` and `epic:browser-host` for context). I ran `aep plan artifact show` on each, plus `relations`, `graph` and `validate`.
- I walked 17 edges, including edges to artifacts outside the set (`story:service-worker-serves-a-specification`, `story:wasm-test-library`, `epic:browser-host`, `vision:spindle`). There is no cycle.
- The graph is a star: each of the four follow-ups depends on `story:node-export-serves-a-specification`, so it does not serialise the set.
- `validate` printed `valid`. It also printed unrelated review-result warnings, which I did not count as findings.
- I traced about 10 acceptance reads to a producer in the set. Each of the four follow-ups reads the lane `check:node_serves_a_specification` that the first story creates, and each has a `depends_on` edge. The release story's read of the lane's first test is covered by the edge but has the gap in the second finding.
- In the code I checked that the fixtures, `Host::advance`, `Seeded`, `ManualClock` and `Surface::fixing_callers` the bodies cite exist. `Surface::report` does not exist yet, which is correct because the story creates it.

What I could not establish:
- Out of my lane (scope or parallel-safety): `story:node-export-fixed-actor` promises `run`'s refusal reasons. Those live in `crates/spindle-cli/src/serve.rs:151-196`, which its scope (`spindle-embed`, `spindle-node`) omits.
- Out of my lane (parallel-safety): the fixed-actor and clock-and-seeds follow-ups both extend the `create` config keys in `spindle-embed` and `spindle-node`. They share no edge between them, and the first story says unknown keys are refused.
- The main story folds the `spindle-embed` extraction into the Node export. I judged that a vertical cut, because the extraction leaves the Service Worker tests green.

```findings
- file: .engineering/planning/story/node-export-serves-a-specification.md
  line: 11
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its outcome and follow-ups restate story:wasm-test-library's outcome (an in-process wasm host for tests, isolated per test, with external answers, clock, seeds), yet the body records only informed_by and does not say what that story keeps or whether it is replaced; state the boundary or add a supersedes edge"
- file: .engineering/planning/story/node-export-release-artifact.md
  line: 28
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its acceptance reads the first story's lane test run against the extracted archive, but no body says the lane takes the module path as an input and this story's scope omits Taskfile.yml and crates/spindle-node/tests/node, so half the seam is in neither item"
```

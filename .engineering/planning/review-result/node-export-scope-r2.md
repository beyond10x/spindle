---
format: aep.planning-md/3
id: review-result:node-export-scope-r2
kind: review-result
status: active
title: Plan critic (scope), round 2, on the Node export stories
relations:
- reviews: story:node-export-serves-a-specification
- reviews: story:node-export-fixed-actor
- reviews: story:node-export-clock-and-seeds
- reviews: story:node-export-events
- reviews: story:node-export-release-artifact
revision: 1
---
needs-revision

- story:node-export-serves-a-specification — the epic promises "no file, socket or thread in the module", and acceptance 8 (`the module reads no file and opens no socket`) checks only built-in imports. No item's outcome or acceptance claims "no thread", so that part of the promise is narrowed with nothing recording the decision. The body should either name a thread check (for example, no `Worker` or `worker_threads` use and no atomics or shared memory in the glue and `.wasm`) or say the clause is dropped. — .engineering/planning/story/node-export-serves-a-specification.md:124, against the epic's "no file, socket or thread in the module" at .engineering/planning/epic/node-export.md:16

The other six promises and the other constraints are each claimed exactly once, with no overlap. Round 1's five findings (the parent never named the Node export) are fixed: `epic:node-export` now states the request and lists the five stories.

What I read: 9 artifacts, plus the round-1 record. I ran `aep plan artifact show` on `epic:node-export` (before any story), the five stories, `review-result:node-export-scope-r1`, `story:wasm-test-library` and `story:host-opening-is-transport-free`. I also ran `aep plan artifact graph` and `grep -n thread` on the epic and story files. From the epic I extracted 14 promises: the 7 requested items plus 7 constraints. The constraints are the four-part "no file, socket or thread" and in-memory state, several instances, Node 22 ES module, release artifact, size in the release notes, the refusal "before listening", and the `wasm-test-library` boundary. I traced 13 to an item. The one left is "no thread".

What I could not establish:
- Story 1's Outcome (line 3) and Design move the Service Worker's opening into `spindle-embed`, `run`'s world reader into `spindle-host`, and the not-served lines into `spindle-ingress`. No epic sentence asks for that refactor, but it is how the epic's "as `run` gives" and "router of the native HTTP host" are met, so I did not call it reach. Whether the refactor is right belongs to the design critic. `story:host-opening-is-transport-free` names the move as story 1's and claims only the WASI remainder, so there is no overlap.
- The release story's gate on the Node lane's tests, its `SHA256SUMS` coverage and its TypeScript declarations go slightly past the epic's "built by the release as an artifact with the module's size in the release notes". They are means to that outcome, so I left them out. This is outside my lane.

```findings
- file: .engineering/planning/story/node-export-serves-a-specification.md
  line: 124
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the epic promises \"no file, socket or thread in the module\" (.engineering/planning/epic/node-export.md:16) and acceptance 8 checks only file and socket (built-in imports); no item claims \"no thread\", so that part of the promise is silently narrowed"
```

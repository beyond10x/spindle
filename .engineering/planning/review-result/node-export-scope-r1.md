---
format: aep.planning-md/3
id: review-result:node-export-scope-r1
kind: review-result
status: active
title: Plan critic (scope), round 1, on the Node export stories
relations:
- reviews: story:node-export-serves-a-specification
- reviews: story:node-export-fixed-actor
- reviews: story:node-export-clock-and-seeds
- reviews: story:node-export-events
- reviews: story:node-export-release-artifact
revision: 1
---
needs-revision

- story:node-export-serves-a-specification — nothing in epic:browser-host asks for a Node-hosted wasm host: the Outcome promises "answers ... from a Service Worker, with state in browser storage", and neither the Outcome nor the four listed stories mention Node, an in-process `create`/`handle` API or a fake backend for Node programs. The epic needs this named, or the story belongs under a different parent — .engineering/planning/epic/browser-host.md:15-18
- story:node-export-fixed-actor — it claims fixed actors for a Node module, which is not traceable to any sentence of the parent, and it stays unclaimed until the parent names the Node export — .engineering/planning/epic/browser-host.md:15-18
- story:node-export-clock-and-seeds — it claims a host-controlled clock, `Host::advance` timers and seeded minting for a Node module, which the parent does not ask for — .engineering/planning/epic/browser-host.md:15-18
- story:node-export-events — it claims event delivery to a host program (callback or pull), which the parent does not ask for — .engineering/planning/epic/browser-host.md:15-18
- story:node-export-release-artifact — it claims a Node module built in the release workflow, with its size in the release notes. That reaches into the release process, and the parent says nothing about Node or release artifacts — .engineering/planning/epic/browser-host.md:15-18

The five stories cover the consumer's seven requested items and its constraints, each claimed once, with no overlap and no narrowing. The consumer's order is kept: the first story holds the not-served report, reset with and without a world, decisions and answer-default, several instances, and `dispose()`. Fixed actor, clock and timers, events and the release artifact are follow-ups. The problem is only the parent. The epic's Outcome, Stories and Rules describe a browser Service Worker. The set is parented to it and does not extend it.

Existing precedent exists: `story:wasm-test-library` is also under this epic, is not in the parent's list, and is not about a Service Worker. Fixing the parent once, by adding the Node export to the Outcome and Stories or by moving the set under a new epic, clears all five findings.

What I read: 7 artifacts (the epic, the five stories, and `story:wasm-test-library` for precedent), via `aep plan artifact show` for each, `aep plan artifact graph`, and `cat -n .engineering/planning/epic/browser-host.md`. I extracted 4 promises from the parent, the Service Worker answering routes, browser storage, the decisions file and the wasm build. All 4 trace to stories outside this set. None trace to this set, and 5 of 5 items in the set have no traceable promise.

What I could not establish, all outside my lane:
- The first story depends on `story:service-worker-serves-a-specification`, which is still draft. Whether that edge is needed is a design question.
- Whether the first story's refactor of `spindle-wasm` and `spindle-cli` into `spindle-embed` carries an acceptance that the Service Worker host is unchanged. That is an acceptance and design question.

```findings
- file: .engineering/planning/epic/browser-host.md
  line: 15
  category: scope
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "story:node-export-serves-a-specification claims a Node-hosted in-process wasm host (create/handle/reset/dispose) that the parent's Service Worker outcome does not mention; extend the epic or re-parent the story"
- file: .engineering/planning/epic/browser-host.md
  line: 15
  category: scope
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "story:node-export-fixed-actor claims fixed actors for a Node module, which is not traceable to any sentence of the parent"
- file: .engineering/planning/epic/browser-host.md
  line: 15
  category: scope
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "story:node-export-clock-and-seeds claims a host-controlled clock, Host::advance timers and seeded minting for a Node module, which the parent does not ask for"
- file: .engineering/planning/epic/browser-host.md
  line: 15
  category: scope
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "story:node-export-events claims event delivery to a host program (callback or pull), which the parent does not ask for"
- file: .engineering/planning/epic/browser-host.md
  line: 15
  category: scope
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "story:node-export-release-artifact claims a Node module built in the release workflow with its size in the release notes, which the parent does not ask for"
```

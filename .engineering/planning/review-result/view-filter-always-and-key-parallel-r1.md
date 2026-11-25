---
format: aep.planning-md/3
id: review-result:view-filter-always-and-key-parallel-r1
kind: review-result
status: active
title: Plan critic (parallel safety), round 1, on the always and key filter story
relations:
- reviews: story:view-filter-always-and-key
revision: 1
---
needs-revision

- story:view-filter-always-and-key — lands on `crates/spindle-host/src/view.rs` with `story:cross-component-view-refused` (`unsupported`), `story:view-read-families` and `story:view-rows-absent-optional-fields`, and names none of them: add a Shared files section with an ordering edge or split the surface.
- story:view-filter-always-and-key — shares `CHANGELOG.md` `[Unreleased]` (cited) and the status page generated from `crates/spindle-docs/src/status.rs` (inferred) with the active `story:ess-099-path-types-and-wrapped-views`; say so, record an ordering edge, or move the docs lines.

`story:command-with-unpublished-event-served` touches no file of this story. The fixture and test modules are unique to it.

```findings
- file: .engineering/planning/story/view-filter-always-and-key.md
  line: 19
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "this story and story:cross-component-view-refused, story:view-read-families and story:view-rows-absent-optional-fields all land on crates/spindle-host/src/view.rs and this body names none of them: add a Shared files section with an ordering edge that records view.rs as the reason, or split the surface"
- file: .engineering/planning/story/view-filter-always-and-key.md
  line: 15
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "this story and the active story:ess-099-path-types-and-wrapped-views both edit CHANGELOG.md [Unreleased] and the capabilities page generated from crates/spindle-docs/src/status.rs; this body does not say so"
```

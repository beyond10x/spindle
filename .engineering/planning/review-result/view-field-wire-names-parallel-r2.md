---
format: aep.planning-md/3
id: review-result:view-field-wire-names-parallel-r2
kind: review-result
status: active
title: Plan critic (parallel safety), round 2, on the view wire names story
relations:
- reviews: story:view-field-wire-names
revision: 1
---
needs-revision

Round 1 is fixed on all findings: `CHANGELOG.md`, `answer.rs` and the test registration are named with their order, the absent `Optional` ordering is stated, and the overlap with `story:wire-names-everywhere` is carved out.

- story:view-field-wire-names — the work edits `website/docs/concepts/capabilities.md`, which neither the scope nor Shared files names, while the active `story:ess-099-path-types-and-wrapped-views` names the capabilities page.

```findings
- file: .engineering/planning/story/view-field-wire-names.md
  line: 69
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the work edits website/docs/concepts/capabilities.md, which the scope and Shared files do not name, though story:ess-099-path-types-and-wrapped-views names the capabilities page"
```

---
format: aep.planning-md/3
id: review-result:view-field-wire-names-parallel-r1
kind: review-result
status: active
title: Plan critic (parallel safety), round 1, on the view wire names story
relations:
- reviews: story:view-field-wire-names
revision: 1
---
needs-revision

- story:view-field-wire-names — `CHANGELOG.md` `[Unreleased]` is shared with three active stories and `answer.rs` is the surface of the active `story:ess-099-path-types-and-wrapped-views`; the body names neither nor who lands first.
- story:view-field-wire-names — the acceptance reads the host's answer for an absent `Optional`, which `story:view-rows-absent-optional-fields` changes, with no edge or sentence ordering the two.
- story:view-field-wire-names — `story:wire-names-everywhere` also claims a view's rows under wire names; neither body mentions the other.

No code file is shared with `story:view-filter-always-and-key` beyond `CHANGELOG.md`; the test registration in `crates/spindle-ingress/tests/spindle-ingress/main.rs` is not in the scope.

```findings
- file: .engineering/planning/story/view-field-wire-names.md
  line: 10
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "CHANGELOG.md [Unreleased] is shared with active view-filter-always-and-key, ess-099 and node-export-serves-a-specification, and answer.rs is the surface of active ess-099; the body has no shared-files section naming either or who lands first"
- file: .engineering/planning/story/view-field-wire-names.md
  line: 69
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the acceptance reads the host's answer for an absent Optional, which story:view-rows-absent-optional-fields changes, and no depends_on edge or body sentence orders the two"
- file: .engineering/planning/story/wire-names-everywhere.md
  line: 16
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "story:wire-names-everywhere also claims a view's rows answered under wire names; neither body mentions the other, so one change could mark both done"
```

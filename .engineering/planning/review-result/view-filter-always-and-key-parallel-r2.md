---
format: aep.planning-md/3
id: review-result:view-filter-always-and-key-parallel-r2
kind: review-result
status: active
title: Plan critic (parallel safety), round 2, on the always and key filter story
relations:
- reviews: story:view-filter-always-and-key
revision: 1
---
needs-revision

Round 1 is fixed in substance: the Shared files section names the three view drafts on `crates/spindle-host/src/view.rs` and `story:ess-099-path-types-and-wrapped-views` on `CHANGELOG.md` and the status page, with an order.

- story:view-filter-always-and-key — `story:view-paging` is a fourth draft on `view.rs` (the paging refusal in `unsupported`), not named.
- story:view-filter-always-and-key — the active `story:node-export-serves-a-specification` also lists `CHANGELOG.md`, not named.

```findings
- file: .engineering/planning/story/view-filter-always-and-key.md
  line: 77
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Shared files list for crates/spindle-host/src/view.rs omits story:view-paging, which lifts the paging refusal in unsupported"
- file: .engineering/planning/story/view-filter-always-and-key.md
  line: 81
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the CHANGELOG.md bullet omits the active story:node-export-serves-a-specification, which lists CHANGELOG.md in scope"
```

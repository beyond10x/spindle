---
format: aep.planning-md/3
id: review-result:view-filter-always-and-key-scope-r1
kind: review-result
status: active
title: Plan critic (scope), round 1, on the always and key filter story
relations:
- reviews: story:view-filter-always-and-key
revision: 1
---
needs-revision

- story:view-filter-always-and-key — the outcome admits `never` and `always` anywhere a leaf is read, while the request asks only for an `(always)` leaf inside a quantifier; narrow the body or mark the wider admission as a deliberate extra.
- story:view-filter-always-and-key — the outcome generalises `key.<field>` to every struct field and deeper member segments, compared, listed and defined; narrow the body or mark the rest as an extra.
- story:view-filter-always-and-key — the fixture view `Named` (`never or key.name == "alpha"`) proves the `never` extra; drop it with the admission or keep it as a marked extra.

All five promises of the request (the `(always)` leaf, the key path, the interpreter's rows, `unsupported` refusing what ESS does not define, the routes no longer 501) are claimed by the story.

```findings
- file: .engineering/planning/story/view-filter-always-and-key.md
  line: 24
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the outcome admits never and always anywhere a leaf is read, but the request asks only for an (always) leaf inside a quantifier; narrow the body or mark the wider admission as a deliberate extra"
- file: .engineering/planning/story/view-filter-always-and-key.md
  line: 28
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the outcome generalises key.<field> to every struct field and deeper member segments, compared, listed and defined, while the request names only the identity key path; narrow the body or mark the rest as an extra"
- file: .engineering/planning/story/view-filter-always-and-key.md
  line: 58
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the fixture view Named and its scenario prove the never extra the request did not ask for; drop them with the never admission or keep them as a marked extra"
```

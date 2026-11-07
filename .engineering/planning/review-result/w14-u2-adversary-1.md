---
format: aep.planning-md/3
id: review-result:w14-u2-adversary-1
kind: review-result
status: active
title: Adversary pass 1 on ingress-observe-panel (wave 14)
relations:
- reviews: story:ingress-observe-panel
revision: 1
---
Adversary pass 1 on `story:ingress-observe-panel`, wave 14 unit U2. Recorded from the adversary's
report.

The adversary could not break the Rust side: self-containment of the built page, the CSP and
framing headers, reserved paths (`/__spindle/ui/`, `//__spindle/ui`, query strings), reads behind
a mount point, the 503 path and `build.rs` rebuild tracking. Two findings in the UI code, both
fixed in the unit (1a79aef); the adversary's cases (`ui/src/panel/adversary.test.ts`) pass
unchanged.

```findings
- file: ui/dev/inline.ts
  line: 10
  category: boundary
  severity: warning
  verdict: INFEASIBLE
  origin: introduced
  message: the inliner escaped only </script, so inlined code holding <!-- then <script would turn the page body into script text; fixed by escaping both
- file: ui/src/panel/Panel.vue
  line: 20
  category: concurrency
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: load() kept whichever answer arrived last, so an older unfiltered read could replace the not-served list; fixed by dropping answers of older calls
```

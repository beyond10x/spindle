---
format: aep.planning-md/3
id: review-result:w13-u3-adversary-1
kind: review-result
status: active
title: Adversary pass 1 on service-worker-serves-a-specification (wave 13)
relations:
- reviews: story:service-worker-serves-a-specification
revision: 1
---
Adversary pass 1 on `story:service-worker-serves-a-specification`, wave 13 unit U3, implementation
ed31e63; the adversary's tests are committed as 4aea8ce. Recorded from the adversary's report.

```
verdict: NEEDS-CHANGE
findings: 4 (1 blocker, 2 warning, 1 note; the note pre-existing)
```

1. `crates/spindle-wasm/src/worker.rs:153`: a bound 205 outcome was built as a Response with a body;
   the browser refuses it and the fetch rejects (test
   `adversary_an_outcome_bound_to_205_is_answered_205`). Warning.
2. `web/src/sw.ts:54`: no test ran the Service Worker shell's decision (origin check, whether to
   answer). Blocker.
3. `web/src/sw.ts:24`: a rejected open stayed cached. Warning.
4. `website/docs/concepts/environment-and-admission.md:74`: a stale sentence. Note, pre-existing.

All fixed in 52a8e57: null-body statuses send no body; the decision moved to Rust
`Spindle::serves(url, method)` with browser test
`the_worker_answers_its_own_origins_routes_and_nothing_else`; the opening is cleared on rejection
(untested, TypeScript); the doc rewritten. Browser lane: 6 passed.

```findings
[
  {"file": "crates/spindle-wasm/src/worker.rs", "line": 153, "category": "contract-drift", "severity": "warning", "verdict": "NEEDS-CHANGE", "origin": "undecided", "message": "A bound 205 outcome is built as a Response with a body; the browser refuses it and the fetch rejects."},
  {"file": "web/src/sw.ts", "line": 54, "category": "acceptance", "severity": "blocker", "verdict": "NEEDS-CHANGE", "origin": "undecided", "message": "No test ran the Service Worker shell's decision (origin check, whether to answer)."},
  {"file": "web/src/sw.ts", "line": 24, "category": "boundary", "severity": "warning", "verdict": "NEEDS-CHANGE", "origin": "undecided", "message": "A rejected open stayed cached."},
  {"file": "website/docs/concepts/environment-and-admission.md", "line": 74, "category": "judgement", "severity": "note", "verdict": "NEEDS-CHANGE", "origin": "pre-existing", "message": "A stale sentence."}
]
```

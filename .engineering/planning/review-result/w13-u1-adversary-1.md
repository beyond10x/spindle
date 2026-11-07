---
format: aep.planning-md/3
id: review-result:w13-u1-adversary-1
kind: review-result
status: active
title: Adversary pass 1 on ingress-request-log (wave 13)
relations:
- reviews: story:ingress-request-log
revision: 1
---
Adversary pass 1 on `story:ingress-request-log`, wave 13 unit U1, implementation 4070220; the
adversary's tests are committed as c5ac089. Recorded from the adversary's report.

```
verdict: NEEDS-CHANGE
findings: 2, both blocker
```

1. `crates/spindle-ingress/src/router.rs:159` and `listener.rs:350`: a 400 for an undecodable path
   variable was recorded with no route, operation or kind (test
   `a_malformed_path_variable_is_recorded_with_its_route`).
2. `crates/spindle-ingress/src/observe.rs:54`, root `surface.rs:376`: `GET //__spindle/x` reached a
   `/{a}/{b}/x` route because `Route::carried` stripped every leading slash (test
   `a_double_slash_never_carries_the_reserved_segment_into_a_route`).

Both fixed in 212b090; `observe::` 15 of 15 pass.

```findings
[
  {"file": "crates/spindle-ingress/src/router.rs", "line": 159, "category": "acceptance", "severity": "blocker", "verdict": "NEEDS-CHANGE", "origin": "undecided", "message": "A 400 for an undecodable path variable is recorded with no route, operation or kind (also listener.rs:350)."},
  {"file": "crates/spindle-ingress/src/observe.rs", "line": 54, "category": "boundary", "severity": "blocker", "verdict": "NEEDS-CHANGE", "origin": "undecided", "message": "GET //__spindle/x reaches a /{a}/{b}/x route because Route::carried strips every leading slash (root surface.rs:376)."}
]
```

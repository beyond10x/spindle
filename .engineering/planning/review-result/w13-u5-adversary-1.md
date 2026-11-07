---
format: aep.planning-md/3
id: review-result:w13-u5-adversary-1
kind: review-result
status: active
title: Adversary pass 1 on browser-store-several-tabs (wave 13)
relations:
- reviews: story:browser-store-several-tabs
revision: 1
---
Adversary pass 1 on `story:browser-store-several-tabs`, wave 13 unit U5. Recorded from the
adversary's report; it gave two defects, without file, line or severity, so no findings block is
written.

1. A reopen after a refused write must read the database afresh.
2. `events()` must recheck health after its read.

Both fixed in c362d7e. Browser lane: 12 passed. The ignore on
`a_second_handle_creating_a_held_identity_is_a_conflict` is removed, as
`verification-report:release-0.3.0` asked. The residual is filed as draft
`story:browser-store-stale-write-conflict`.

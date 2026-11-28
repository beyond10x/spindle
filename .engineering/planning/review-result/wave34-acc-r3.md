---
format: aep.planning-md/3
id: review-result:wave34-acc-r3
kind: review-result
status: active
title: Plan critic (acceptance), round 3, on wave 34
relations:
- reviews: story:release-artifacts-signed
- reviews: story:unknown-commit-answered-as-unknown
- reviews: story:reset-on-a-busy-store-keeps-the-store
revision: 1
---
approve

No findings.

Round 2's one finding is resolved. In `story:reset-on-a-busy-store-keeps-the-store`, the Acceptance case `a_reset_whose_first_step_is_busy_keeps_the_supplied_store` now runs the `forget_events` leg on a store that holds no row. It cites `host.rs:1955-1960` for the order (removals before `forget_events`). This matches the tree: `Host::empty` calls `removed_all` and then `forget_events`, and `reset_rows` is set only when a removal lands. With no row held, `reset_changed_rows()` stays `false` on that leg. The `scan` and first-`remove` legs keep a store holding one row, and "the row (where the store held one) is still read" now fits both. The case's name and the wave page's four-new-tests count are unchanged and still match the Acceptance bullets.

Round 1's finding, the missing stub case for the backfill identity (`adversary_backfill_verifies_at_main` in `story:release-artifacts-signed`), was already confirmed resolved in round 2 and is unchanged.

I found nothing the revision broke. The round 2 diff touches only the `story:reset-on-a-busy-store-keeps-the-store` body, the wave page's `## Revision round 2` line and the round 2 review records. Each story still has an observable check:
- **`story:unknown-commit-answered-as-unknown`:** four tests, each failing or not compiling before the work.
- **`story:reset-on-a-busy-store-keeps-the-store`:** four new tests plus the un-ignored one, each with a before-state and an exit status.
- **`story:release-artifacts-signed`:** six stub cases, plus the real checks recorded in the tagged 0.29.0 run's step summary.

The story bodies and the wave page name no person, organisation, host or home path (grep found none). No new Python appears. Test budgets are stated (1 s per test, about 1.6 s and 2 s added, the lanes unchanged).

What I read: 3 of 3 ids (`story:release-artifacts-signed`, `story:unknown-commit-answered-as-unknown`, `story:reset-on-a-busy-store-keeps-the-store`), each with `aep plan artifact show`. I also read the wave page including `## Revision round 1` and `## Revision round 2`, `review-result:wave34-acc-r2`, and `host.rs` (`empty`, `removed_all`) in the worktree.

What I could not establish: how real `cosign` and `gh attestation verify` behave. Only the tagged 0.29.0 run shows that, and the story says so.

Out of my lane, not counted in the verdict: the image's provenance attestation is made in `sign` (D5 A), and no stub case covers it beyond the 40-hex pin check.

```findings
[]
```

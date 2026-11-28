---
format: aep.planning-md/3
id: review-result:wave34-acc-r2
kind: review-result
status: active
title: Plan critic (acceptance), round 2, on wave 34
relations:
- reviews: story:release-artifacts-signed
- reviews: story:unknown-commit-answered-as-unknown
- reviews: story:reset-on-a-busy-store-keeps-the-store
revision: 1
---
needs-revision

story:reset-on-a-busy-store-keeps-the-store — the Acceptance case `a_reset_whose_first_step_is_busy_keeps_the_supplied_store` cannot pass as written for its `forget_events` leg. `Host::empty` calls `removed_all` before `forget_events`, so on "a supplied store holding one row" that row is already removed, `reset_rows` is true and the row is no longer read. The sentence asks for `reset_changed_rows()` false and "the row is still read" on that leg. The fix is for the bullet to say the `forget_events` leg runs on a store that holds no row. — `.engineering/planning/story/reset-on-a-busy-store-keeps-the-store.md:74-78` ("on a supplied store holding one row, with a busy `scan`, a busy first `remove` and a busy `forget_events` in turn … `Host::reset_changed_rows()` is `false`, the row is still read"); tree: `crates/spindle-host/src/host.rs:1955-1960` (`removed_all` first, then `forget_events`) and `:3023-3025` (`*removed = true` after a landed removal).

Round 1's one finding is resolved. `story:release-artifacts-signed` now has the stub case `adversary_backfill_verifies_at_main`. It requires a `--certificate-identity` ending in `@refs/heads/main` on `workflow_dispatch` and in `@refs/tags/<tag>` on a tag push, and it fails on either swap. That matches D6 A and the Outcome.

The revision broke nothing else I could find:
- U1's guard-form list holds against the tree. `image-manifest`'s `if:` at `release.yml:691` matches the first form, and `publish`'s at `:766` matches the second, with its `||` only inside parentheses.
- The 16 and 14 upload counts match the `publish` upload step at `release.yml:839-861`, where `*.cdx.json` does not match the new bundle names.
- U2's four tests, and U3's tests as the wave page counts them, each fail or cannot compile before the work and are observable.
- The story bodies and the wave page contain no person, organisation, host or home path.

What I read: 3 of 3 ids (`story:release-artifacts-signed`, `story:unknown-commit-answered-as-unknown`, `story:reset-on-a-busy-store-keeps-the-store`), each with `aep plan artifact show`. I also read the wave page, `review-result:wave34-acc-r1`, `release.yml` (guards, upload step), `host.rs` (`empty`, `removed_all`) and `listener.rs:163-174`, and grepped the story files for personal names, hosts and home paths.

What I could not establish:
- Whether real `cosign` and `gh attestation verify` behave as the story writes them. Only the tagged 0.29.0 run shows that, and the story says so.
- Out of my lane, not counted in the verdict: the image's provenance attestation is made in `sign` (D5 A), while no stub case covers the attestation step beyond the 40-hex pin check. Only the tagged run observes it.

```findings
[
  {
    "file": ".engineering/planning/story/reset-on-a-busy-store-keeps-the-store.md",
    "line": 74,
    "category": "acceptance",
    "severity": "warning",
    "verdict": "needs-revision",
    "origin": "introduced",
    "message": "the case a_reset_whose_first_step_is_busy_keeps_the_supplied_store puts a busy forget_events on 'a supplied store holding one row' and expects reset_changed_rows() false and the row still read, but Host::empty runs removed_all before forget_events, so that row is already removed and reset_changed_rows() is true; the bullet must run the forget_events leg on a store that holds no row"
  }
]
```

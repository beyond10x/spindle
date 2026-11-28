---
format: aep.planning-md/3
id: review-result:wave34-acc-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on wave 34
relations:
- reviews: story:release-artifacts-signed
- reviews: story:unknown-commit-answered-as-unknown
- reviews: story:reset-on-a-busy-store-keeps-the-store
revision: 1
---
needs-revision

story:release-artifacts-signed — the Outcome promises that a backfill (`workflow_dispatch`) is signed and verified at `refs/heads/main`, and D6 A repeats it. No acceptance case observes that. The five stub cases never run the signing or verify step as a `workflow_dispatch`. The tagged run of 0.29.0 is a tag push, so `every_release_asset_verifies` only ever checks the tag-ref identity. The acceptance should add a stub case that runs the verify step with `EVENT=workflow_dispatch` and asserts the `--certificate-identity` it passes ends in `@refs/heads/main`. Otherwise the backfill sentence should leave the Outcome. Quoted sentence: "or at `refs/heads/main` for a backfill (`workflow_dispatch`)" — `.engineering/planning/story/release-artifacts-signed.md:39`, with D6 at `:135`.

**What I read:** 3 of 3 ids (`story:release-artifacts-signed`, `story:unknown-commit-answered-as-unknown`, `story:reset-on-a-busy-store-keeps-the-store`), plus the wave page. I ran `aep plan artifact show` on each and `aep plan artifact kinds` and `lifecycle story`. I also checked the cited tree lines in the worktree: `release.yml` job guards, the existing script cases at `:632-634` and `:722-771`, `listener.rs:163-174`, `answer.rs:582-587`, `adversary_w33_u2.rs:528-531` (the `#[ignore]`), `sqlite.rs:3015` and the `Taskfile.yml` task. I scanned the story bodies and the wave page for personal names, organisations, hosts and home paths and found none.

**What holds:**
- U2 and U3 name observable tests that fail before the work: no `StoreError::Unknown` exists yet, and the parked case is ignored. Their time budgets are stated and under 1 s per test.
- U1's stub cases are checkable and make no network call. A pull-request run of `release.yml` is kept from `id-token: write` by `adversary_id_token_only_on_signing_jobs`. The real verification is correctly placed in the tagged run, with its step summary as the story's evidence.

**What I could not establish:**
- Whether the real `cosign` and `gh attestation verify` commands behave as written. Only the 0.29.0 tagged run shows that, and the story's own "Uncertain" lines say so.
- Out of my lane, not affecting my verdict: `story:reset-on-a-busy-store-keeps-the-store`'s case `a_reset_whose_store_fails_otherwise_still_loses_it` needs U2's `StoreError::Unknown`. It cannot compile on U3's own branch, and there is no `depends_on` edge between the units. That belongs to the parallel-safety or design critic.

```findings
[
  {
    "file": ".engineering/planning/story/release-artifacts-signed.md",
    "line": 39,
    "category": "acceptance",
    "severity": "warning",
    "verdict": "needs-revision",
    "origin": "introduced",
    "message": "the Outcome (and D6 A, :135) promises that a backfill (workflow_dispatch) is signed and verified at refs/heads/main, but no stub case and no tagged-run check observes the backfill identity; the acceptance names only the tag's ref, so it reads the same whether or not a backfill signs correctly; add a stub case that runs the verify step with EVENT=workflow_dispatch and asserts the certificate identity ends in @refs/heads/main, or drop the backfill sentence from the Outcome"
  }
]
```

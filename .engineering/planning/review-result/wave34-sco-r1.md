---
format: aep.planning-md/3
id: review-result:wave34-sco-r1
kind: review-result
status: active
title: Plan critic (scope), round 1, on wave 34
relations:
- reviews: story:release-artifacts-signed
- reviews: story:unknown-commit-answered-as-unknown
- reviews: story:reset-on-a-busy-store-keeps-the-store
revision: 1
---
approve

I extracted four promises from the parent and traced all four, and I found nothing to change. The parent is the brief and the wave page's `## Selection`, because the pick is not a stored artifact.

The four promises are:
1. D-W34 A picks `story:release-artifacts-signed`, which means no release asset is signed today. The set claims it in U1's `## Outcome`, where each archive, the SBOM, `IMAGES` and `SHA256SUMS` carry a bundle and a provenance attestation, and the image is signed by its digest.
2. The owner's yes for `id-token: write` and `attestations: write` on the release workflow. U1's `## Decided when scoped` D4 and D5 and its `adversary_id_token_only_on_signing_jobs` acceptance claim it. That acceptance also keeps both permissions off any pull-request run.
3. D-W34 A picks `story:unknown-commit-answered-as-unknown`, which says an unknown commit answers `501` with `committed: false`. U2 claims it in `## Outcome` (`500` with `"committed": "unknown"`, `StoreError::Unknown`).
4. D-W34 A picks `story:reset-on-a-busy-store-keeps-the-store`, which says a busy reset loses the store. U3 claims it in `## Outcome` (`503` while no row changed, `500` with the store kept after).

The exclusions hold:
- **Seed and retry paths:** U2's D4 A and its `## Outcome` name them as `story:unknown-commit-in-seed-and-retries`. That draft exists and serves `vision:spindle`, so this is a recorded omission, not a gap.
- **Excluded stories:** `story:sensitive-fields-redacted` and `story:feed-wait-under-load` are named in the wave page as not picked.

I found no story reaching beyond its promise. U1's extra surfaces (`Taskfile.yml`, a `verify` mode in `scripts/release-linux-binary.sh`) are marked inferred and conditional, and they serve the cited outcome. U3's `serve.rs` (comment only), `rows.rs` and `answer.rs` (D1 B only) entries are inferred and not claimed by any acceptance.

No two stories claim the same outcome. U3's `a_reset_whose_store_fails_otherwise_still_loses_it` uses U2's `StoreError::Unknown` only to classify a reset. U2 claims no reset behaviour.

No promise is narrowed. U1's backfill and image cases are decided (D6, D3, D5) and carried into its acceptance.

The privacy check on the three bodies and the wave page found no person, organisation, host or home path. The `actions/attest-build-provenance` pin and the `token.actions.githubusercontent.com` issuer are technical names the workflow needs.

**What I read:**
- Three stories, with `aep plan artifact show story:<id>` for each.
- `.engineering/waves/wave-34.md`.
- The `aep plan artifact graph` edges for them.
- A `grep` of the story bodies and wave page for personal strings and home paths.
- `git grep` at `a6dc0280` for `id-token` under `.github`.

**What I could not establish:**
- The wave page's "Closed with this plan" paragraph moves `story:postgres-lane-margin` and `story:scan-cost-test-steady` to `implemented`. That is outside the three-story set and the brief did not ask for it. I did not read those two stories.
- Out of my lane, for the other critics:
  - **Merge order:** U3's `supplied.rs` test names `StoreError::Unknown`, which exists only on U2's branch. It compiles only after U2 is merged. This is a parallel-safety matter, and the wave page's merge order already covers it.
  - **Checkability:** the image attestation is made by `actions/attest-build-provenance`, which no stub case can run. Only its pin is stub-checked, and the tagged 0.29.0 run is its evidence.

```findings
[]
```

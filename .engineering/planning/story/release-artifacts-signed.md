---
format: aep.planning-md/3
id: story:release-artifacts-signed
kind: story
status: draft
title: Release artifacts are signed and carry a provenance attestation
summary: Every release archive, the SBOM and the image carry a cosign signature and a SLSA provenance attestation.
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- informed_by: story:spindle-image-published
- informed_by: story:release-binary-portable-libc
scope:
- confidence: cited
  path: .github/workflows/release.yml
- confidence: inferred
  path: AGENTS.md
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: README.md
- confidence: inferred
  path: Taskfile.yml
- confidence: inferred
  path: scripts/release-linux-binary.adversary-test.sh
- confidence: inferred
  path: scripts/release-linux-binary.sh
revision: 6
---
# Release artifacts are signed and carry a provenance attestation

## Outcome

Every asset a release publishes (each archive, the SBOM, `IMAGES` and `SHA256SUMS`) carries a
cosign keyless signature, uploaded beside it as `<asset>.sigstore.json`, and a SLSA build
provenance attestation in the repository's attestations; the image's manifest list is signed by its
digest and attested the same way. A consumer verifies each with `cosign verify-blob`,
`cosign verify` and `gh attestation verify` before it deploys. The signing identity is the
release workflow at the tag's ref, or at `refs/heads/main` for a backfill (`workflow_dispatch`).

## Why

Line numbers at 18715a80 (`release.yml` is unchanged since 276298bb).

- The release publishes five archives with a `.sha256` each (`.github/workflows/release.yml:389`),
  the SBOM with its `.sha256` (`:465`) and, for a commit with a Containerfile, `IMAGES` with its
  `.sha256` (`:753`); `publish` checks them and merges them into `SHA256SUMS` (`:798-824`), then
  uploads the archives, the SBOM, `IMAGES` and `SHA256SUMS` (`:839-861`). Nothing signs them. The
  workflow's permissions are `contents: read` at the top (`:19-20`), `packages: write` on
  `image-push` and `image-manifest` (`:589`, `:697`) and `contents: write` on `publish` (`:771`);
  no job holds `id-token: write`. A checksum beside an asset proves nothing about who built it.
- The image is published (`story:spindle-image-published` is implemented): `image-manifest`
  (`:689-762`) writes the manifest list's digest to `IMAGES` (`:748-750`). buildx runs with
  `--provenance=false --sbom=false` (`:536`, `:638`, `:662`), which the reproducibility check
  needs, so the image's provenance is attached by digest after the push.
- A job named `provenance` exists (`:32`); it verifies the tag. The signing job is named `sign`.
- `story:release-binary-portable-libc` is archived; this story adds only the signatures.

## Acceptance

Stub cases, in `scripts/release-linux-binary.adversary-test.sh`, run by
`task check:the_release_binary_checks_refuse_what_they_must` (part of `task check` and of CI) on
stub tools: no case signs, pulls, pushes or reaches a network. Each case is named in its `fail`
message. The script stays under 60 s; the unit's report names its time before and after.

- `adversary_id_token_only_on_signing_jobs`: `id-token: write` appears in `release.yml` on the
  `sign` and `image-manifest` jobs only, `attestations: write` on `sign` only, the top-level
  permissions stay `contents: read`, and each job holding `id-token: write` has an `if:` that is
  false on a `pull_request` event. The case checks this against a fixed list of accepted guard
  forms, which it names: the `if:` is `${{ github.event_name != 'pull_request' }}`, or starts with
  `${{ github.event_name != 'pull_request' && ` (as `image-manifest`'s, `release.yml:691`) or
  `${{ !cancelled() && github.event_name != 'pull_request' && ` (as `publish`'s, `:766`), and has
  no `||` outside parentheses after that start; each such form is false on a `pull_request`
  event. A copy of `release.yml` with `id-token: write` at the top level or on `build`, with
  `sign`'s pull-request guard removed, or with `|| true` appended to it, fails the case.
- `adversary_sign_signs_each_asset`: `sign`'s signing step, run on a stub `cosign` over a release
  with and without `IMAGES`, writes one `<asset>.sigstore.json` per asset (five archives, the SBOM,
  `IMAGES` where expected, `SHA256SUMS`) and calls `cosign sign-blob` once per asset; a stub that
  fails on one asset fails the step.
- `adversary_publish_uploads_a_bundle_per_asset`: `publish`'s upload step on the `gh` stub uploads
  each asset and its bundle (16 files with `IMAGES`, 14 without) and refuses, as today, an asset
  already on the page. The existing checksum and upload cases (`:722-771`) move to the new counts
  and pass.
- `adversary_image_signed_by_manifest_digest`: `image-manifest`'s signing step on a stub `cosign`
  signs `<image>@sha256:<digest>` read from `IMAGES`, never a tag; a step that signs
  `<image>:<version>` fails the case.
- `adversary_pinned_tools_are_checked`: each step that fetches `cosign` checks it with
  `sha256sum --check --strict` against the pinned sha256 before it runs it, so a stub download with
  one byte changed stops the step; every `uses:` of `actions/attest-build-provenance` names a
  40-hex commit.
- `adversary_backfill_verifies_at_main`: `publish`'s verification step, run on a stub `cosign`
  that records its arguments, passes a `--certificate-identity` ending in `@refs/heads/main` when
  run with `GITHUB_EVENT_NAME=workflow_dispatch` (a backfill, D6 A), and one ending in
  `@refs/tags/<tag>` on a tag push; a step that passes the tag's ref on a backfill, or
  `refs/heads/main` on a tag push, fails the case.

The tagged release run: the first release built with this story (0.29.0) runs a verification step
in `publish` after the upload, on the assets downloaded back from the release page. It writes each
command and its output to the run's step summary, and fails the run if a check fails. That summary
is the story's evidence (`test_result`), recorded when the story closes.

- `every_release_asset_verifies`: each asset verifies with
  `cosign verify-blob --bundle <asset>.sigstore.json --certificate-identity <the release workflow at the tag's ref> --certificate-oidc-issuer https://token.actions.githubusercontent.com`,
  and `gh attestation verify <asset> --repo <repository> --signer-workflow <repository>/.github/workflows/release.yml`
  passes and names the tagged commit.
- `a_changed_archive_fails_verification`: a copy of one archive with one byte changed fails both
  commands; the step fails if either passes.
- `the_image_is_signed_when_published`: `cosign verify <IMAGES line>` with the same identity
  verifies the manifest list's digest, and `gh attestation verify oci://<IMAGES line>` passes.

Documents: `AGENTS.md` release step 7 (`:269-274`) lists the verify commands, `README.md`
§ Releases (`:279-291`) tells a consumer how to verify an asset and the image, and the change goes
under `[Unreleased]` `### Added` in `CHANGELOG.md`. No Rust test is added; the default test step
and the PostgreSQL lane are unchanged.

## Decided when scoped

Taken 2026-11-28 on the operator's pick (D-W34 A): the scoper's default, option A, of every
decision, each reversible. The owner's yes of 11:08 CET covers the workflow permissions
`id-token: write` and `attestations: write`.

- D1 Provenance: A, `actions/attest-build-provenance` (needs `attestations: write`), pinned to
  v4.2.2 at commit `4d101475d8b20a2381f78447822ac1eab6504dd8`; the pin is checked as the commit the
  action's tag `v4.2.2` points to (`GET /repos/actions/attest-build-provenance/git/ref/tags/v4.2.2`).
  Not taken: B (slsa-github-generator), C (`cosign attest-blob`).
- D2 cosign on the runner: A, the release binary `cosign-linux-amd64` of cosign v3.1.6, sha256
  `318a99ade4156a0c4113aa983b46c0a0830fa7896a096c63121ee47b610a94c6`, checked with
  `sha256sum --check --strict` before use, as the workflow checks cargo-cyclonedx (`:427`); the sha
  is the line for that file in the version's published `cosign_checksums.txt`, itself signed
  (`cosign_checksums.txt.sigstore.json`). The same pinned step runs in `sign`, `image-manifest` and
  `publish`. `gh` is the runner image's own; nothing else is added.
  Not taken: B (`sigstore/cosign-installer` pinned by SHA).
- D3 What is signed: A, each archive, the SBOM, `IMAGES` and `SHA256SUMS`, one
  `<asset>.sigstore.json` each.
  Not taken: B (only `SHA256SUMS`), C (archives and SBOM).
- D4 Where signing runs: A, a new job `sign` before `publish`, holding `id-token: write` and
  `attestations: write`, never on a pull request. It takes over `publish`'s checksum step
  (`:798-824`) so `SHA256SUMS` exists before it is signed, and hands `SHA256SUMS` and the bundles to
  `publish` as an artifact; `publish` keeps `contents: write` and gains no permission.
  Not taken: B (inside `publish`).
- D5 Image signing: A, in `image-manifest`, by the manifest list's digest, which gives that job
  `id-token: write` beside its `packages: write`; the image's provenance attestation is made in
  `sign` by the same digest, kept in the repository's attestations (no push to the registry).
  Not taken: B (a separate job).
- D6 Backfills: A, signed too; verification accepts the tag's ref or `refs/heads/main` as the
  identity's ref.
  Not taken: B (backfills unsigned).
- D7 Test coverage: A, stub `cosign` in the test script; the real verification runs in the tagged
  release run and its step summary is the evidence.
  Not taken: B (a recorded bundle verified offline).

## Scope

Derived 2026-11-28 by `story-scoper` on `plan/wave-33` (276298bb). Every line is **cited** (read
from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `.github/workflows/release.yml` — cited
- **Files:** `.github/workflows/release.yml:19-20` (top-level `permissions: contents: read`), `:389` (archive `.sha256`), `:465` (SBOM `.sha256`), `:689-762` (`image-manifest`, manifest digest at `:748`), `:764-861` (`publish`; `SHA256SUMS` built at `:798-824`, file count at `:813`, upload at `:839-861`) — cited (the story's `:280-281` and `:356` were stale)
- **Symbols:** jobs `build`, `sbom`, `image-manifest`, `publish`; new job `sign` (not `provenance`, which is taken at `:32`) — inferred
- **Also likely:** `scripts/release-linux-binary.adversary-test.sh` (stub cases; its check at `:632-634` that `packages: write` appears on two jobs only and at `:722-771`, the checksum and upload steps on stubs) — inferred
- **Also likely:** `scripts/release-linux-binary.sh` (a `verify` mode wrapping `cosign verify-blob`, if one is added; it is already in `on.pull_request.paths`, `release.yml:13-17`) — inferred
- **Also likely:** `Taskfile.yml:51-54` (`check:the_release_binary_checks_refuse_what_they_must` desc) — inferred
- **Documents:** `AGENTS.md` release steps 6-7 (`:219-274`, the verify commands in step 7 at `:269-274`), `README.md` § Releases (`:279-291`), `CHANGELOG.md` Unreleased — inferred
- **Confidence:** high for `release.yml`; medium for the script and doc lines
- **Would collide with:** any unit touching the release workflow, its stub test script or the release section of `AGENTS.md`/`README.md`; among wave-34 units only `CHANGELOG.md` is shared
- **Safety fact:** the jobs holding `id-token: write` (`sign`, `image-manifest`) run on no pull request, so a pull-request run of `release.yml` (triggered at `:13-17`) cannot mint a certificate under the release workflow's identity — unproven; `adversary_id_token_only_on_signing_jobs` checks it
- **Checked when planned (2026-11-28, read against 18715a80):** `release.yml` is unchanged since 276298bb and its lines above hold; moved elsewhere: the `packages: write` check `:632-634` (was `:634`), the stub checksum and upload cases `:722-771`, the `SHA256SUMS` step `:798-824` (was `:794-824`), the file count `:813` (was `:814`), `AGENTS.md` steps 6-7 `:219-274` (was `:230-262`), `README.md` § Releases `:279-291` — cited. D5 A puts `id-token: write` on `image-manifest` as well as `sign` — cited from the decision. cosign v3.1.6 and `actions/attest-build-provenance` v4.2.2 are the newest releases on 2026-11-28; the scoper named v3.0.x and v3.x — cited (the projects' release pages)
- **Uncertain:** whether the repository's allowed-actions setting admits `actions/attest-build-provenance`; ghcr.io's referrers API against `.sig` tags for the image signature; the test script's current run time (not measured); uploading the bundles to a release page that is already published

---
format: aep.planning-md/3
id: story:release-artifacts-signed
kind: story
status: draft
title: Release artifacts are signed and carry a provenance attestation
summary: Every release archive, the SBOM and the image carry a cosign signature and a SLSA provenance attestation.
tags:
- needs-timo-acceptance
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- informed_by: story:spindle-image-published
- informed_by: story:release-binary-portable-libc
revision: 1
---
# Release artifacts are signed and carry a provenance attestation

## Outcome

Every archive and SBOM a release publishes, and the image once `story:spindle-image-published`
ships, carries a cosign signature and a SLSA provenance attestation a consumer verifies before it
deploys.

## Why

The release publishes archives with a SHA-256 file each and an SBOM with its SHA-256
(`.github/workflows/release.yml:280-281`, `release.yml:356`), and nothing signs them: the workflow
holds no signing or attestation step. A checksum published beside an archive proves nothing about
who built it. The image (`story:spindle-image-published`) and the glibc floor of the binary
(`story:release-binary-portable-libc`) are drafted already; this story adds only the signatures.

## Acceptance

- `every_release_asset_verifies`: for a release, `cosign verify-blob` with the workflow's identity
  verifies each archive and the SBOM, and a provenance attestation for each names the tagged commit
  and the release workflow.
- `a_changed_archive_fails_verification`: an archive with one byte changed fails it.
- `the_image_is_signed_when_published`: once the image is published, `cosign verify` with the same
  identity verifies its digest.
- The release's verification report records the verify commands and their output.

## Needs Timo's acceptance

Keyless signing gives the release job `id-token: write`, a change to the workflow's permissions.

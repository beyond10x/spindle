---
format: aep.planning-md/3
id: story:spindle-image-published
kind: story
status: draft
title: The release publishes a Spindle image with its digest
relations:
- serves: vision:spindle
- decomposes: epic:kubernetes-target
- depends_on: story:ingress-bind-address
- informed_by: story:release-binary-portable-libc
scope:
- confidence: cited
  path: .github/workflows/release.yml
- confidence: inferred
  path: Containerfile
- confidence: cited
  path: README.md
- confidence: cited
  path: website/docs
revision: 4
---
# The release publishes a Spindle image with its digest

## Outcome

Each release builds and pushes `ghcr.io/beyond10x/spindle:<version>`, one manifest list for
`linux/amd64` and `linux/arm64`, holding the `b10x-spindle` binary the release already builds for
`x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` (`.github/workflows/release.yml:160-165`),
on a minimal base, as a non-root user, with `b10x-spindle` as the entrypoint. The manifest list's
digest is attached to the release as `IMAGES` (`<image>@sha256:<digest>`) beside `SHA256SUMS`
(`release.yml:269-298`) and printed in the release notes.

## Acceptance

- The release job fails when the image's `b10x-spindle --version` does not print the release's
  version, on either architecture.
- The release's assets hold `IMAGES`, and the digest in it resolves to a manifest list with both
  architectures.
- The image's configured user is not root (UID other than 0).
- With `story:ingress-bind-address`'s option, `run` in the container answers a request sent from
  outside the container to the published port.

## Scope

- `.github/workflows/release.yml` (the build matrix, lines 154-236; the upload, lines 244-298).
- A container build file at the repository root (new; for example `Containerfile`), copying the
  built binaries rather than building inside the image.
- `README.md` and `website/docs/` (where to pull the image).

## Notes

`story:console-image-published` edits the same workflow; they do not run beside each other.

## Consumer gap

Recorded 2026-11-24. A downstream consumer running a specification on 0.19.1 in a container has
no published image: every user writes a Dockerfile that downloads the release archive and checks
it against `SHA256SUMS`. This story is that gap; its acceptance already asks for a versioned image
with its digest in the release notes.

The image's base must run the binary the release builds: today the Linux binaries need
`GLIBC_2.39` (built on `ubuntu-24.04`), so a Debian bookworm or distroless base (glibc 2.36)
fails. A static base is open only after `story:release-binary-portable-libc`.

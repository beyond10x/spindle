---
format: aep.planning-md/3
id: story:console-image-published
kind: story
status: draft
title: The release publishes the observability console image with its digest
tags:
- needs-timo-acceptance
relations:
- serves: vision:spindle
- decomposes: epic:kubernetes-target
- depends_on: story:spindle-image-published
- depends_on: story:console-server
scope:
- confidence: cited
  path: .github/workflows/release.yml
- confidence: inferred
  path: Containerfile.console
- confidence: inferred
  path: crates/spindle-console
revision: 4
---
# The release publishes the observability console image with its digest

**Needs Timo's acceptance**: a second shipped image.

## Outcome

Each release also builds and pushes `ghcr.io/beyond10x/spindle-console:<version>`, multi-arch like
the Spindle image, holding `b10x-spindle-console` (`story:console-server`) as a non-root user, with
one exposed port and no volume. Its digest is listed in the release's `IMAGES` beside the Spindle
image's.

## Acceptance

- The release job fails when the image's `b10x-spindle-console --version` does not print the
  release's version.
- `IMAGES` lists both images.
- The console image's digest in `IMAGES` resolves to a manifest list with both architectures.
- The image's configured user is not root.
- The image declares exactly one exposed port.
- Started with a read-only root file system and `--prometheus` and `--model` URLs, the container
  answers `GET /` on its port.

## Scope

- `.github/workflows/release.yml` (build and upload).
- `Containerfile.console` (new, at the repository root).
- `crates/spindle-console/` (built for both Linux targets).

## Shared files

`story:spindle-image-published` edits `.github/workflows/release.yml` too; this story runs after it.

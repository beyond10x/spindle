---
format: aep.planning-md/3
id: story:release-binary-portable-libc
kind: story
status: draft
title: The Linux release binary runs on an older glibc
summary: The Linux release archives run on Debian bookworm (glibc 2.36), or are static musl builds, checked in CI.
relations:
- serves: vision:spindle
revision: 1
---
# The Linux release binary runs on an older glibc

## Outcome

The `b10x-spindle` archives the release publishes for `x86_64-unknown-linux-gnu` and
`aarch64-unknown-linux-gnu` run on Debian bookworm (glibc 2.36), or are replaced by static musl
builds that run on any Linux, and CI checks that on every release build.

## Why

A downstream consumer running 0.19.1 in a container found that the Linux release binary needs
`GLIBC_2.39` and fails to start on Debian bookworm, whose glibc is 2.36. The release builds both
Linux targets on `ubuntu-24.04` and `ubuntu-24.04-arm` (`.github/workflows/release.yml`, the
`build` job's matrix), so the binary links against the runner's glibc 2.39. Every user on an older
distribution, or a container on a bookworm base, builds from source or picks a newer base image.
`story:spindle-image-published` would inherit the same floor.

## Acceptance

- `the_linux_archives_run_on_bookworm`: the release workflow runs each Linux archive's
  `b10x-spindle --version` inside a `debian:bookworm` container (the arm64 one on an arm64 runner)
  and fails when it does not print the release's version.
- `the_glibc_floor_is_named`: the highest `GLIBC_` symbol version the binary needs is printed in
  the build log, and the release fails above the floor the README names; or, with musl, the binary
  is static (`ldd` says so) and the targets are renamed in the archives, `SHA256SUMS`, the SBOM
  check and the README.
- `pull_requests_check_it_too`: a pull request that changes the workflow runs the same check, as
  it runs the builds.

## Decided when scoped

- musl (static; SQLite and TLS must build for it, and the allocator's speed is measured against
  the glibc build) or a glibc baseline from an older build image (`cargo zigbuild`, or a manylinux
  or Debian bookworm container).

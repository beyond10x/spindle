---
format: aep.planning-md/3
id: story:release-linux-glibc-floor-and-musl
kind: story
status: draft
title: The Linux release runs on an old glibc and as a static binary
relations:
- serves: vision:spindle
revision: 1
---
# The Linux release runs on an old glibc and as a static binary

## Outcome

Each release publishes, beside today's archives, an `x86_64-unknown-linux-gnu` binary built against
glibc 2.17 and an `x86_64-unknown-linux-musl` static archive. A host on an older glibc (a build node
runs glibc 2.34) runs `b10x-spindle` without the warning or failure a binary linked against a newer
glibc gives it.

Ordered on 2026-11-25 for every beyond10x Rust tool (static musl Linux releases, mimalloc where
allocation is heavy, consumers pin the musl archives). Refined on 2026-11-26 from ESS's
measurement: musl with mimalloc ran its synthesis workload 82 % slower than gnu, so ESS builds gnu
against glibc 2.17 (cargo-zigbuild, checked with `objdump -T`) and keeps musl as the static
fallback; the shared composite action takes the 2.17 floor as an option.

## Acceptance

- The release workflow uploads `b10x-spindle-<v>-x86_64-unknown-linux-gnu` built on the glibc 2.17
  floor, and a CI check reads its symbol versions with `objdump -T` and fails on any `GLIBC_` above
  2.17.
- It uploads `b10x-spindle-<v>-x86_64-unknown-linux-musl.tar.gz`; a CI check confirms the binary is
  static and runs `--version` in a plain Alpine container.
- One Spindle workload (a served specification under a fixed request run) is timed on the gnu and
  the musl binary side by side, with and without mimalloc; mimalloc is kept only where it is not
  slower, and the measurement is written in this story before musl is named a default anywhere.
- `SHA256SUMS` covers the new archives.

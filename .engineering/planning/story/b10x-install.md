---
format: aep.planning-md/3
id: story:b10x-install
kind: story
status: draft
title: b10x setup and upgrade install b10x-spindle from its release
relations:
- serves: vision:spindle
revision: 1
---
# b10x setup and upgrade install b10x-spindle from its release

## Outcome

`b10x init`, `b10x setup apply` and `b10x upgrade` can install and upgrade `b10x-spindle` from the
release assets of `beyond10x/spindle`, checksum-verified against `SHA256SUMS`, the way they install
`ess` and `aep`. Spindle's side: release archives named and laid out as the installer expects,
documented in `AGENTS.md`; the installer's side (a product entry in the `b10x` CLI) is a change in the
repository that owns `b10x`, requested from there.

## Acceptance

- The 0.x release assets match the archive name, layout and `SHA256SUMS` format the `b10x` installer
  reads for `ess` and `aep` (checked against that installer's source).
- `b10x upgrade spindle --host claude --out <plan>` names the newest spindle release and its
  install action, and applying it puts a `b10x-spindle` on `PATH` whose `--version` is that release.
- `README.md` names `b10x` as the install route beside the release page.

## Origin

The operator, 2026-11-06, relayed by `b10x-chef-orchestrator` from another session: "spindle should
deliver prebuild binaries in its gh CI for easier installation/download". Linux archives exist since
0.1.0; macOS is `story:macos-host-builds`.

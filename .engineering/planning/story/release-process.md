---
format: aep.planning-md/3
id: story:release-process
kind: story
status: implemented
title: A release process for spindle
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: .github/workflows/release.yml
- confidence: cited
  path: AGENTS.md
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: Cargo.toml
- confidence: cited
  path: README.md
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T10:36:24Z", actor: "agent:b10x-chef-orchestrator", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-06T10:36:24Z", actor: "agent:b10x-chef-orchestrator", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-06T13:06:14Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":1}}}
---
# A release process for spindle

## Outcome

spindle is released from `main`: a `## [X.Y.Z]` section in `CHANGELOG.md`, the workspace version in
`Cargo.toml`, a tag `X.Y.Z` pushed by `b10x-bot[bot]`, and a release workflow that builds
`b10x-spindle` for Linux (x86_64 and aarch64; macOS is story:macos-host-builds) and attaches the archives with `SHA256SUMS`
to a GitHub release authored by `b10x-bot[bot]`. The crates stay `publish = false`; nothing is
published to a registry. `AGENTS.md` documents the steps. The shape follows the release workflows
and `AGENTS.md` release sections of `connectors` (`release-binaries.yml`) and `takt`
(`release.yml`).

## Acceptance

- `.github/workflows/release.yml` (or the name the shape uses) runs on a pushed `X.Y.Z` tag, builds
  the two Linux targets and uploads `b10x-spindle-X.Y.Z-<target>.tar.gz` plus `SHA256SUMS`.
- `AGENTS.md` § Publishing states the release steps; `README.md` links them for humans.
- The first release is cut from `main` after wave 9 merges and verified with the orchestrator's
  `verify-release.sh spindle <tag>` (exit 0).

## Origin

The operator, 2026-11-06, relayed by `b10x-chef-orchestrator` with the date: "So for spindle, I
mean, I don't know, if it doesn't has a release process, give it a release process, please. We do
want to release it."

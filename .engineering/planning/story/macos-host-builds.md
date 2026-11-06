---
format: aep.planning-md/3
id: story:macos-host-builds
kind: story
status: implemented
title: b10x-spindle builds and runs on macOS
relations:
- serves: vision:spindle
- depends_on: story:release-process
scope:
- confidence: cited
  path: .github/workflows/release.yml
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T14:45:47Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-06T14:45:47Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-06T19:57:15Z", actor: "human:timo", revision: 5, decided_on: {"recorded":{"test_result":1}}}
---
# b10x-spindle builds and runs on macOS

## Outcome

`b10x-spindle` compiles for `x86_64-apple-darwin` and `aarch64-apple-darwin`, the in-process
commands (`check`, `capabilities`, `admit`, `run`, `conform`, `replay`) work there, and the release
workflow adds both macOS targets. Linux-only parts (vsock telemetry, microVM launchers, the
parent-death signal) are compiled only on Linux or answer a declared refusal elsewhere.

Today `cargo check -p spindle-target --target aarch64-apple-darwin` exits 101:
`crates/spindle-telemetry/src/sys.rs:20` and `src/vsock.rs:189,193` use `libc::SOCK_CLOEXEC` and
`libc::accept4`; `crates/spindle-target/src/process.rs:156` uses a parent-death signal (inferred
Linux-only, not compiled).

## Acceptance

- `cargo check --workspace --target aarch64-apple-darwin` and `x86_64-apple-darwin` succeed in CI.
- A macOS runner job runs the in-process test lanes green.
- `release.yml` lists both macOS targets and the release carries their archives in `SHA256SUMS`.

## Origin

Found while writing `story:release-process` (2026-11-06); the first release ships Linux only.

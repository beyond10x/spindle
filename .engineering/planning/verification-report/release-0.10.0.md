---
format: aep.planning-md/3
id: verification-report:release-0.10.0
kind: verification-report
status: accepted
title: Release 0.10.0 verified
relations:
- verifies: vision:spindle
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-10T16:13:55Z", actor: "human:timo", revision: 2}
---
# Release 0.10.0 verified

## Result

- Tag `0.10.0` is on `2d78103e`, the merge of PR https://github.com/beyond10x/spindle/pull/43
  (release commit `40a3e9a1`). It is annotated (tag object `3b4e927e`), and `b10x-bot[bot]` tagged
  and pushed it.
- Wave 18 was delivered as PR https://github.com/beyond10x/spindle/pull/42, merged as `82faeb87`
  (head `9940078c`). CI 42837182265 (Gate and macOS): success.
- Runs on `2d78103e`, all success: CI 42848172709, Shared source gates 42848173944 and
  42848666828, Documentation validation 42848172748, Documentation site 42848650174.
- Release run https://github.com/beyond10x/spindle/actions/runs/42848666124: success, 8 jobs.
- `b10x-bot[bot]` created the page https://github.com/beyond10x/spindle/releases/tag/0.10.0, and
  the latest-release API answers `0.10.0`. The page carries
  `b10x-spindle-0.10.0-{x86_64,aarch64}-unknown-linux-gnu.tar.gz`,
  `b10x-spindle-0.10.0-{aarch64,x86_64}-apple-darwin.tar.gz` and `SHA256SUMS`.
- The downloaded archives pass `sha256sum --check SHA256SUMS` (4 OK). The x86_64 Linux binary
  prints `b10x-spindle 0.10.0`.
- Conformance: 79 executed, 79 passed, 0 skipped, with no known-failures declaration.
- Local gate on the release tree: every step through conformance, the wasm build, the wasm memory
  store, the static bundle, fmt, clippy and `cargo test --workspace` (56 suites) exit 0. Two browser
  steps (IndexedDB, service worker) exit 201 on this host, which has no WebDriver; the local
  `wasm-bindgen-test-runner` is 0.2.129, so the checks ran with 0.2.130 from the release archive
  CI pins. CI's Gate runs them, and they pass there.

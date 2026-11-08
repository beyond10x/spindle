---
format: aep.planning-md/3
id: verification-report:release-0.8.0
kind: verification-report
status: accepted
title: Release 0.8.0 verified
relations:
- verifies: vision:spindle
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-08T17:40:09Z", actor: "human:timo", revision: 2}
---
# Release 0.8.0 verified

## Result

- Tag `0.8.0` on `e6c1fd98` (merge of PR https://github.com/beyond10x/spindle/pull/36, release
  commit `3c78ccfb`), annotated, tagged and pushed by `b10x-bot[bot]`.
- Local gate: every `task check` step passed on `e6c1fd98` (the same tree as `3c78ccfb`): 892
  tests passed, 34 ignored; conformance 41 executed, 40 passed, 1 failed as declared. One timing
  test, `play::play_stopped_by_a_signal_removes_its_state_and_exits_with_the_signal_status`, failed
  once on the loaded host and passed on rerun (`play::` 19 of 19). The gate built into the tree's
  own `target/`, as a shared build directory breaks the static bundle and nine tests.
- Wave 16 delivered as PR https://github.com/beyond10x/spindle/pull/35, merged as `1a3879aa`
  (head `d5254a15`). Runs on `d5254a15`: CI 42499657344 (Gate and macOS), Shared source gates
  42499657744, Documentation validation 42499657522: success.
- Release run https://github.com/beyond10x/spindle/actions/runs/42504861368: success.
- The bot-created page https://github.com/beyond10x/spindle/releases/tag/0.8.0 is Latest and
  carries `b10x-spindle-0.8.0-{x86_64,aarch64}-unknown-linux-gnu.tar.gz`,
  `b10x-spindle-0.8.0-{aarch64,x86_64}-apple-darwin.tar.gz` and `SHA256SUMS`.
- The downloaded archives pass `sha256sum --check SHA256SUMS` (4 OK); the x86_64 Linux binary
  prints `b10x-spindle 0.8.0`.
- Runs on `e6c1fd98`: CI 42501499602, Shared source gates 42501499960 and 42504861600,
  Documentation validation 42501499659, Documentation site 42501758822: success.
- Release diff 0.7.0..0.8.0: no adversary test removed. The
  `#[ignore = "story:surface-cross-component-template-collision"]` added in 0.7.0 was removed by
  that story.

## Follow-up

- None.

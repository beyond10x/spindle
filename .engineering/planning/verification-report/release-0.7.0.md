---
format: aep.planning-md/3
id: verification-report:release-0.7.0
kind: verification-report
status: accepted
title: Release 0.7.0 verified
relations:
- verifies: vision:spindle
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-08T14:31:15Z", actor: "human:timo", revision: 2}
---
# Release 0.7.0 verified

## Result

- Tag `0.7.0` on `b900fac7` (merge of PR https://github.com/beyond10x/spindle/pull/33, release
  commit `a8f13230`), annotated, tagged and pushed by `b10x-bot[bot]`.
- Local gate: every `task check` step passed on `a8f13230` (837 workspace tests passed, 0 failed,
  35 ignored; conformance with its one declared known failure). The static bundle and the
  workspace tests were rerun built into the tree's own `target/` after a shared build directory
  broke them.
- Wave 15 delivered as PR https://github.com/beyond10x/spindle/pull/32, merged as `82c4fb1a`
  (head `2bda8cd8`). Runs on `2bda8cd8`: CI 42465501089 (attempt 5, Gate and macOS), Shared source
  gates 42465501535 (attempt 8), Documentation validation 42465501085: success. The earlier
  attempts failed only on GitHub release downloads answering 503 or 504 (tool installs), never on
  the code.
- Release run https://github.com/beyond10x/spindle/actions/runs/42488679389: success.
- The bot-created page https://github.com/beyond10x/spindle/releases/tag/0.7.0 is Latest and
  carries `b10x-spindle-0.7.0-{x86_64,aarch64}-unknown-linux-gnu.tar.gz`,
  `b10x-spindle-0.7.0-{aarch64,x86_64}-apple-darwin.tar.gz` and `SHA256SUMS`.
- The downloaded archives pass `sha256sum --check SHA256SUMS` (4 OK); the x86_64 Linux binary
  prints `b10x-spindle 0.7.0`.
- Runs on `b900fac7`: CI 42488579784, Shared source gates 42488580079 and 42488679858,
  Documentation validation 42488579768, Documentation site 42488870194: success.
- Release diff 0.6.0..0.7.0: no adversary test removed. The ignore at
  `crates/spindle-kernel/tests/spindle-kernel/adversary_row_set.rs:241` was removed by
  `story:kernel-row-set-commit-conflict`. One
  `#[ignore = "story:surface-cross-component-template-collision"]` was added.

## Follow-up

- `story:surface-cross-component-template-collision` is delivered on the open wave 16 branch,
  which removes that ignore.

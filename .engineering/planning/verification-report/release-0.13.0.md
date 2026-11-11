---
format: aep.planning-md/3
id: verification-report:release-0.13.0
kind: verification-report
status: accepted
title: Release 0.13.0 verified
relations:
- verifies: vision:spindle
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-11T07:37:32Z", actor: "human:timo", revision: 2}
---
# Release 0.13.0 verified

## Result

- Tag `0.13.0` is on `134757a2`, the merge of PR https://github.com/beyond10x/spindle/pull/50
  (head `c88213e2`: `18cda67f` concurrent reads, `09066b7a` fix pass 1, the release commit, and
  `c88213e2` running the two `/proc/self/fd` adversary cases on Linux only after the macOS job
  failed them). It is annotated, and `b10x-bot[bot]` tagged and pushed it.
- PR #50 checks on `c88213e2`: Gate (run 42974459003, 8m48s), macOS, Build documentation
  (42974459014), Security and privacy (42974456159): all pass.
- CI on `134757a2` (run 42975555246): success. Release run
  https://github.com/beyond10x/spindle/actions/runs/42975821633: success.
- `b10x-bot[bot]` created https://github.com/beyond10x/spindle/releases/tag/0.13.0 with the four
  `b10x-spindle-0.13.0-*.tar.gz` archives and `SHA256SUMS`; the downloads pass
  `sha256sum --check SHA256SUMS` (4 OK).
- Contents: `story:concurrent-reads`. Debug build, writer held 2 s, median of 5: a view read
  2.0 s before; 1.7 ms (memory) and 2.9 ms (SQLite) after; two 600 ms reads at once 1.20 s
  before, 602 ms after. Review: `review-result:reads-adversary-1`, 5 findings, fixed in
  `09066b7a`.

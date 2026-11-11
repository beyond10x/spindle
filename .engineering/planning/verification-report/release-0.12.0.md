---
format: aep.planning-md/3
id: verification-report:release-0.12.0
kind: verification-report
status: accepted
title: Release 0.12.0 verified
relations:
- verifies: vision:spindle
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-11T02:02:03Z", actor: "human:timo", revision: 2}
---
# Release 0.12.0 verified

## Result

- Tag `0.12.0` is on `2cf3ae91`, the merge of PR https://github.com/beyond10x/spindle/pull/48
  (head `fac37b4b`: the ESS 0.92.0 pin `56815812` and the release commit). It is annotated, and
  `b10x-bot[bot]` tagged and pushed it.
- PR #48 checks on `fac37b4b`: Gate (run 42938791029, 16m18s), macOS, Build documentation
  (42938791166), Security and privacy (42938791500): all pass.
- Release run https://github.com/beyond10x/spindle/actions/runs/42941460653: success.
- `b10x-bot[bot]` created the page https://github.com/beyond10x/spindle/releases/tag/0.12.0 with
  the four `b10x-spindle-0.12.0-*.tar.gz` archives and `SHA256SUMS`; the downloads pass
  `sha256sum --check SHA256SUMS` (4 OK).
- Contents: `story:ess-format-range` part 1. Specifications in `ess/34`, `ess/35` and `ess/36` are
  admitted, served and conformed (crates/spindle-cli/tests/spindle-cli/format_range.rs, red on ESS
  0.90.0 for `ess/36`); conformance 79 of 79.

## Not in this release

- Part 2 of `story:ess-format-range`: ESS 0.92.0 refuses `ess/37` whole
  (`unsupported_format_version`), so reading a newer format needs a forward-reading mode in ESS.

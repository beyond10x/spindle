---
format: aep.planning-md/3
id: verification-report:release-0.11.0
kind: verification-report
status: accepted
title: Release 0.11.0 verified
relations:
- verifies: vision:spindle
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-11T00:52:25Z", actor: "human:timo", revision: 2}
---
# Release 0.11.0 verified

## Result

- Tag `0.11.0` is on `1c24a44f`, the merge of PR https://github.com/beyond10x/spindle/pull/47
  (release commit `e19d48e5`). It is annotated, and `b10x-bot[bot]` tagged and pushed it.
- Wave 19 (U2 `story:http-caller-proves-actor`, U3 `story:binding-where-condition`) was delivered
  as PR https://github.com/beyond10x/spindle/pull/46, merged as `a69a8f46` (head `960a5ef2`).
  Its checks on `960a5ef2`: CI 42929372071 (Gate 8m58s, macOS), Documentation validation
  42929372062, Shared source gates 42929398567: all pass.
- PR #47 checks on `e19d48e5`: Gate (run 42931151845, 8m35s), macOS, Build documentation
  (42931151983), Security and privacy (42931152324): all pass.
- Release run https://github.com/beyond10x/spindle/actions/runs/42932696245: success.
- `b10x-bot[bot]` created the page https://github.com/beyond10x/spindle/releases/tag/0.11.0. It
  carries `b10x-spindle-0.11.0-{x86_64,aarch64}-unknown-linux-gnu.tar.gz`,
  `b10x-spindle-0.11.0-{aarch64,x86_64}-apple-darwin.tar.gz` and `SHA256SUMS`.
- The downloaded archives pass `sha256sum --check SHA256SUMS` (4 OK).
- The release commit was not gated locally: it changes only the version (Cargo.toml, Cargo.lock)
  and CHANGELOG headings, and PR #47's Gate ran `task check` on it.

## Not in this release

- U1 `story:postgres-store` and U4 `story:state-moves-to-postgres`: in fix pass 3 after
  `review-result:w19-u1-adversary-3` and `review-result:w19-u4-adversary-3`.

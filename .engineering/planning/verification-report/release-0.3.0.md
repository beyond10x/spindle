---
format: aep.planning-md/3
id: verification-report:release-0.3.0
kind: verification-report
status: accepted
title: Release 0.3.0 verified; one new ignore accepted (D628)
relations:
- verifies: vision:spindle
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-06T20:11:23Z", actor: "human:timo", revision: 2}
---
# Release 0.3.0 verified; the ignore check counts moved files

## Result

- Tag `0.3.0` on `6d8b523` (merge of PR 24), annotated, tagged and pushed by `b10x-bot[bot]`.
- Release run 42284011236: success; Linux x86_64 and aarch64 and macOS aarch64 and x86_64
  archives and `SHA256SUMS` on the bot-created page
  https://github.com/beyond10x/spindle/releases/tag/0.3.0. The first macOS archives.
- CI run 42277632205 on the merged head: `Gate` and `macOS` success.
- The organisation's release check: 16 ok, 35 FAIL "release diff adds #[ignore]", exit 1.

## Why it failed

The check reads the release diff without rename detection, so the 143 test files that
`story:one-test-binary-per-crate` moved into `tests/<crate>/` read as deleted and added, and each
moved `#[ignore]` reads as added. Counted over the whole tree (`git grep -c '#\[ignore'` over
`*.rs`): 39 at `0.2.0`, 40 at `0.3.0`. The one new ignore is
`crates/spindle-store-browser/tests/spindle-store-browser/adversary.rs:131`, the adversary's
two-handle case, ignored with its reason and owned by `story:browser-store-several-tabs`.

## Decision

`b10x-chef-orchestrator`, 2026-11-06, D628: 0.3.0 stays released with one FAIL on record. The
check now reads the ignore diff with rename detection; its re-run gives 16 ok and 1 FAIL, the
new ignore above, accepted because the crate documents two handles as unsupported and the ignore
names `story:browser-store-several-tabs`. Condition: the next wave that touches
`spindle-store-browser` takes that story and removes the ignore.

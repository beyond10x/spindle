---
format: aep.planning-md/3
id: verification-report:release-0.6.0
kind: verification-report
status: accepted
title: Release 0.6.0 verified
relations:
- verifies: vision:spindle
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-07T18:05:43Z", actor: "human:timo", revision: 2}
---
# Release 0.6.0 verified

## Result

- Tag `0.6.0` on `8930f63` (merge of PR https://github.com/beyond10x/spindle/pull/29), annotated,
  tagged and pushed by `b10x-bot[bot]`; an ancestor of `origin/main`.
- First Release run https://github.com/beyond10x/spindle/actions/runs/42394812368: provenance,
  Gate green on the tagged commit, observe UI and the four target builds green; the upload job
  failed at "Create the checksum manifest". The publish job downloads the artifacts matching
  `*-*-*`; the panel artifact `ui-dist-panel` added in this release matched too, so `dist/` held
  five entries. Nothing was uploaded.
- Fix: PR https://github.com/beyond10x/spindle/pull/30 names the panel artifact `ui-panel`; merged
  as `1960b50`.
- Backfill run https://github.com/beyond10x/spindle/actions/runs/42398200165 (`workflow_dispatch`,
  tag `0.6.0`, the tag unmoved): success on its first attempt, every job green.
- The bot-created page https://github.com/beyond10x/spindle/releases/tag/0.6.0 is Latest and
  carries `b10x-spindle-0.6.0-{x86_64,aarch64}-unknown-linux-gnu.tar.gz`,
  `b10x-spindle-0.6.0-{aarch64,x86_64}-apple-darwin.tar.gz` and `SHA256SUMS`.
- The downloaded archives pass `sha256sum --check SHA256SUMS` (4 OK); the x86_64 Linux binary
  prints `b10x-spindle 0.6.0`.
- Runs on `8930f63`: CI 42394692083, Documentation validation 42394692051, Documentation site
  42395018890, Shared source gates 42394692479 and 42394812626: success.
- Release diff 0.5.0..0.6.0: no adversary test removed; one `#[ignore]` added, at
  `crates/spindle-kernel/tests/spindle-kernel/adversary_row_set.rs:241` (a row committed by a
  second writer between the scan and the commit is not detected).

## Follow-up

- The next wave that touches `crates/spindle-kernel/src/store.rs` takes
  `story:kernel-row-set-commit-conflict` (draft, needs acceptance) and removes that ignore.

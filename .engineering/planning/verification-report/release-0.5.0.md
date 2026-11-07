---
format: aep.planning-md/3
id: verification-report:release-0.5.0
kind: verification-report
status: accepted
title: Release 0.5.0 verified
relations:
- verifies: vision:spindle
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-07T06:37:57Z", actor: "human:timo", revision: 2}
---
# Release 0.5.0 verified

## Result

- Tag `0.5.0` on `fe72629` (merge of PR https://github.com/beyond10x/spindle/pull/27), annotated,
  tagged and pushed by `b10x-bot[bot]`; equal to `origin/main` at verification.
- Release run https://github.com/beyond10x/spindle/actions/runs/42346447993: success on the first
  attempt, every job green (provenance, Gate green on the tagged commit, observe UI, four target
  builds, upload).
- The bot-created page https://github.com/beyond10x/spindle/releases/tag/0.5.0 is Latest and
  carries `b10x-spindle-0.5.0-{x86_64,aarch64}-unknown-linux-gnu.tar.gz`,
  `b10x-spindle-0.5.0-{aarch64,x86_64}-apple-darwin.tar.gz` and `SHA256SUMS`.
- The downloaded archives pass `sha256sum --check SHA256SUMS` (4 OK); the x86_64 Linux binary
  prints `b10x-spindle 0.5.0`.
- Runs on `fe72629` and the tag: CI 42346353015, Documentation validation 42346352973,
  Documentation site 42346666228, Shared source gates 42346353279 and 42346448298: success.
- Release diff 0.4.0..0.5.0: no `#[ignore]` added, no adversary test removed; the ignore on
  `a_second_handle_creating_a_held_identity_is_a_conflict` is gone, as
  `verification-report:release-0.3.0` asked.

## Follow-up

- Draft `story:browser-store-stale-write-conflict`: across real tabs a stale write is refused late,
  not with a synchronous `Conflict`.
- The gate's format step covers workspace members only: the ESS 0.89.0 type generator writes the
  out-of-workspace `crates/spindle-observe-api/types.rs` in a form rustfmt changes.

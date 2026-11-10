---
format: aep.planning-md/3
id: verification-report:release-0.9.0
kind: verification-report
status: accepted
title: Release 0.9.0 verified
relations:
- verifies: vision:spindle
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-10T07:04:34Z", actor: "human:timo", revision: 2}
---
# Release 0.9.0 verified

## Result

- Tag `0.9.0` is on `ed70f2ee`, the merge of PR https://github.com/beyond10x/spindle/pull/40
  (release commit `718ba770`, store close `0ae827fb`). It is annotated, and `b10x-bot[bot]`
  tagged and pushed it.
- Wave 17 was delivered as PR https://github.com/beyond10x/spindle/pull/39, merged as `9cd4ccbc`
  (head `3934d733`). CI 42768444426 (Gate and macOS): success.
- Runs on `ed70f2ee`, all success: CI 42770815516, Shared source gates 42770816716 and
  42770972723, Documentation validation 42770815452, Documentation site 42771066709.
- Release run https://github.com/beyond10x/spindle/actions/runs/42770972341: success, 8 jobs.
- `b10x-bot[bot]` created the page https://github.com/beyond10x/spindle/releases/tag/0.9.0, and the
  latest-release API answers `0.9.0`. The page carries
  `b10x-spindle-0.9.0-{x86_64,aarch64}-unknown-linux-gnu.tar.gz`,
  `b10x-spindle-0.9.0-{aarch64,x86_64}-apple-darwin.tar.gz` and `SHA256SUMS`.
- The downloaded archives pass `sha256sum --check SHA256SUMS` (4 OK). The x86_64 Linux binary
  prints `b10x-spindle 0.9.0`.
- Conformance: 79 executed, 79 passed, 0 skipped, with no known-failures declaration. 0.8.0 had 41
  executed, 40 passed and 1 failed as declared.
- Local gate on the release tree: the steps through the static bundle, fmt and clippy exit 0. Two
  browser steps (IndexedDB, service worker) exit 201 on this host; the runner exits abnormally, and
  the host has no WebDriver. CI's Gate runs them, and they pass there.

## Follow-up

- story:look-ahead-asks-only-where-it-matters: its 4 adversary cases are ignored until it lands.

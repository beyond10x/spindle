---
format: aep.planning-md/3
id: verification-report:release-0.2.0
kind: verification-report
status: accepted
title: Release 0.2.0 verified, one check failed and accepted (D625)
relations:
- verifies: vision:spindle
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-06T16:19:52Z", actor: "human:timo", revision: 2}
---
# Release 0.2.0 verified, one check failed and accepted (D625)

## Result

- Tag `0.2.0` on `1260bb3` (merge of PR 22), annotated, tagged and pushed by `b10x-bot[bot]`.
- Release run 42248608169: success; Linux x86_64 and aarch64 archives and `SHA256SUMS` on the
  bot-created release page https://github.com/beyond10x/spindle/releases/tag/0.2.0.
- The organisation's release check: 16 ok, 1 FAIL, exit 1. The FAIL, verbatim: "release diff
  removes adversary test adversary_an_unroutable_view_refuses_startup_by_name in
  crates/spindle-cli/tests/adversary_unserved.rs (8b3e1a9a2a)".

## Why it failed

Commit `8b3e1a9` fixed a defect a consumer was blocked on: a view the ingress could not route
stopped `b10x-spindle run` at startup (wave 9). The adversary test asserted that refusal; the same
hunk renames it `adversary_an_unroutable_view_answers_501_by_name` and asserts the 501. No story
owned the fix, so the hunk names none, and the check requires a `story:` in a hunk that removes an
adversary test.

## Decision

`b10x-chef-orchestrator`, 2026-11-06, D625: "spindle 0.2.0 accepted as released. My re-run shows
the same 16 ok / 1 FAIL; I read 8b3e1a9a2a: the removed adversary test is renamed in the same hunk
and asserts the new 501 contract, so no coverage is lost." The rule that a hunk rewriting an
adversary test names its story is recorded in `AGENTS.md` (Rules).

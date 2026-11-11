---
format: aep.planning-md/3
id: verification-report:release-0.12.1
kind: verification-report
status: accepted
title: Release 0.12.1 verified
relations:
- verifies: vision:spindle
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-11T04:45:55Z", actor: "human:timo", revision: 2}
---
# Release 0.12.1 verified

## Result

- Tag `0.12.1` is on `a82f246e`, the merge of PR https://github.com/beyond10x/spindle/pull/49
  (head `1376c1f4`: the fix `30ed87d9`, its review fix `1673276a`, a merge of main, and the
  release commit). It is annotated, and `b10x-bot[bot]` tagged and pushed it.
- PR #49 checks on `1376c1f4`: Gate (run 42957349387), macOS, Build documentation, Security and
  privacy: all pass.
- CI on `a82f246e` (run 42958509632) first failed one test,
  `adversary_processes::a_peer_slow_to_start_is_reached_within_the_connect_bound`, which passed on
  the PR; the failed job was rerun and passed (Gate and macOS success). Filed as
  `story:peer-connect-test-steady` (draft).
- The release run 42958684456 first stopped at "Gate green on the tagged commit"; rerun after CI
  went green, it succeeded and uploaded the four `b10x-spindle-0.12.1-*.tar.gz` archives and
  `SHA256SUMS` to https://github.com/beyond10x/spindle/releases/tag/0.12.1. The downloads pass
  `sha256sum --check SHA256SUMS` (4 OK).
- Contents: `story:metrics-read-without-scan`. Regression test (500 commands, 100 entities, 5,000
  events, debug): a metrics read 41.7-56.3 s before, 31-45 ms after; a command sent during it
  53-57 s before, 40-73 ms after. Review: `review-result:metrics-adversary-1`, fixed.

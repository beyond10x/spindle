---
format: aep.planning-md/3
id: verification-report:release-0.4.0
kind: verification-report
status: accepted
title: Release 0.4.0 verified after one macOS rerun
relations:
- verifies: vision:spindle
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-07T02:17:37Z", actor: "human:timo", revision: 2}
---
# Release 0.4.0 verified after one macOS rerun

## Result

- Tag `0.4.0` on `2c211a9` (merge of PR 26), annotated, tagged and pushed by `b10x-bot[bot]`;
  equal to `origin/main` at verification.
- Release run 42326722964: first attempt failed in "Gate green on the tagged commit", because
  main's CI run 42326647238 on `2c211a9` was red in the macOS job
  (`adversary_links::a_second_hello_before_the_link_opens_is_refused`,
  `crates/spindle-host/tests/spindle-host/adversary_links.rs:275`; 83 passed, 1 failed). The same
  job had passed on the PR head `058292b`. Under `approval-record:release-0.4.0-macos-red` the
  failed jobs of 42326647238 were rerun through the bot and passed, so it is a timing flake; the
  failed jobs of 42326722964 were rerun and the run succeeded.
- The bot-created page https://github.com/beyond10x/spindle/releases/tag/0.4.0 carries
  `b10x-spindle-0.4.0-{x86_64,aarch64}-unknown-linux-gnu.tar.gz`,
  `b10x-spindle-0.4.0-{aarch64,x86_64}-apple-darwin.tar.gz` and `SHA256SUMS`.
- `verify-release.sh spindle 0.4.0` with the announcement file: exit 0. Runs CI 42326647238,
  Release 42326722964, Documentation site 42326854244, Documentation validation 42326647247,
  Shared source gates 42326647906 and 42326723205: success. Release diff 0.3.0..0.4.0: no
  `#[ignore]` added, no adversary test removed.

## Follow-up

- Draft `story:links-hello-test-timing`: the flaky test waits for its condition instead of a fixed
  300 ms.

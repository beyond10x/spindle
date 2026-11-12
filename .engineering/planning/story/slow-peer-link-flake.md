---
format: aep.planning-md/3
id: story:slow-peer-link-flake
kind: story
status: draft
title: A peer slow to start is reached within the connect bound on every CI run
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

`spindle-cli` `adversary_processes::a_peer_slow_to_start_is_reached_within_the_connect_bound` passes
every CI run. It failed once on the Gate job of PR 55 (run 43122783131, 2026-11-11T23:57Z) and passed
on the rerun and in every local gate:

```
assertion `left == right` failed: exit 1:
{"component":"rally-table","listening":"127.0.0.1:37273","peers":{"rally-player-east":"127.0.0.1:37571","rally-player-west":"127.0.0.1:46261"}}
b10x-spindle: rally-table has no link from rally-player-west (from) within 30s
  left: Some(1)
 right: Some(0)
```

The test is unchanged since a86f229. Find whether the 30 s link bound or the test's peer start
timing fails under CI load, and fix the cause rather than the bound.

## Origin

Wave 23 release, pre-existing, filed by the coordinator.

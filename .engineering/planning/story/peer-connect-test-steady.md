---
format: aep.planning-md/3
id: story:peer-connect-test-steady
kind: story
status: draft
title: The slow-peer connect test passes on a loaded runner
relations:
- serves: vision:spindle
revision: 1
---
# The slow-peer connect test passes on a loaded runner

## Outcome

`adversary_processes::a_peer_slow_to_start_is_reached_within_the_connect_bound` and
`adversary_processes::three_processes_play_the_match_one_process_plays_on_a_fast_clock` pass on a
loaded CI runner and on a loaded workstation, or say in their failure which bound the host missed.

## Why

- CI Gate on main `a82f246e` (run 42958509632, 2026-11-11) failed only this test, at
  crates/spindle-cli/tests/spindle-cli/adversary_processes.rs:520, after the same tree passed on
  its PR (#49). It blocked the 0.12.1 release upload until rerun.
- On the workstation at load 45, both tests failed with "the channel is broken: closed" while
  another session ran the spindle-cli tests (ESS 0.92.0 pin gate, 2026-11-11) and passed on rerun.

## Acceptance

1. The two tests pass 20 runs in a row under `stress-ng --cpu <cores>` on the workstation.
2. A failure names the bound and the measured time.

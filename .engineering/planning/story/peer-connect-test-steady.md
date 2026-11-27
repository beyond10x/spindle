---
format: aep.planning-md/3
id: story:peer-connect-test-steady
kind: story
status: draft
title: The slow-peer connect test passes on a loaded runner
relations:
- serves: vision:spindle
- informed_by: story:slow-peer-link-flake
scope:
- confidence: inferred
  path: crates/spindle-cli/src/component.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/adversary_processes.rs
- confidence: inferred
  path: crates/spindle-host/Cargo.toml
- confidence: inferred
  path: crates/spindle-host/src/host/component.rs
revision: 6
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

1. `a_case_listens_on_a_loopback_host_no_other_process_holds`: with `127.0.0.1:P` held by the test,
   a peer started at `127.a.b.c:P` prints its listening line.
2. `a_connection_that_reached_itself_is_no_link`: a connection whose local and peer address are
   equal is refused by `Host::connect`, which retries.
3. The two tests the Outcome names bound their match below the 60 s kill, and a failure prints the
   bound, the measured time and the console of every process.
4. The two tests pass 20 runs in a row under load on the workstation; this run is manual and stays
   out of the gate.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli` integration tests — cited
- **Files:** `crates/spindle-cli/tests/spindle-cli/adversary_processes.rs:502` (`a_peer_slow_to_start_is_reached_within_the_connect_bound`), `:353` (`three_processes_play_the_match_one_process_plays_on_a_fast_clock`), `:520` — cited
- **Symbols:** `free_addresses`, `Proc::spawn`, `MATCH_WITHIN`, `same_match_as_one_process` in `adversary_processes.rs` — inferred, the helpers both tests share
- **Also likely:** `crates/spindle-host/src/host/component.rs:542` (`Host::connect`: treat a stream whose `local_addr` equals its `peer_addr` as "not listening yet") — inferred
- **Also likely:** `crates/spindle-host/Cargo.toml` (dev-dependency `rustix` with `net`, already in `Cargo.lock`, for a bind-then-connect self-connect test) — inferred
- **Also likely:** `crates/spindle-cli/src/component.rs:144-172` (link errors name the measured wait beside `LINK_WITHIN`) — inferred
- **Documents:** none
- **Confidence:** medium — the test file is cited; the cause (pick-then-drop port, taken by another process or by a self-connect) is read from the code and the archived failure, not reproduced
- **Would collide with:** any unit changing `spindle-cli`'s `adversary_processes.rs` helpers, the link set-up of `spindle-host` `host/component.rs`, or how `examples/rally/ess` is played under `play --component`
- **Safety fact:** every address in `127.0.0.0/8` is local on Linux, and a connect to `127.a.b.c` leaves from `127.0.0.1`. So a case's own loopback host can be taken neither by another case's `127.0.0.1` port nor by a self-connect. `component.rs` checks loopback only for the ingress (`crates/spindle-cli/src/component.rs:249`), not for `listen` or peers — step 2, unproven

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: each case listens on a loopback host of its own (`127.a.b.c`); `MATCH_WITHIN` drops
  below the 60 s kill; a failure prints the bound, the measured time and every console;
  `Host::connect` refuses a connection that reached itself and retries.

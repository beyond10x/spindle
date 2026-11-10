---
format: aep.planning-md/3
id: story:run-one-component
kind: story
status: draft
title: run --component runs one component of any specification in its own process
relations:
- serves: vision:spindle
- decomposes: epic:split-run
scope:
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/component.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-cli/tests/fixtures
- confidence: cited
  path: crates/spindle-host/src/host/component.rs
revision: 2
---
# run --component runs one component of any specification in its own process

## Outcome

`b10x-spindle run <spec> --environment <file> --component <name>` runs one component of any
specification, admitted on its own, its event bindings carried over `channel = tcp`: it listens
on `--listen` for its peers' links and connects to each peer at `SPINDLE_PEER_<COMPONENT>`, as
`play --component` does for rally today (`crates/spindle-cli/src/cli.rs:240-248`). Where the
component is reached over the network it serves its own routes on its ingress, as `run` serves
them; periodic bindings it hosts run on the wall clock. It runs until SIGINT or SIGTERM. The
rally-only parts of `crates/spindle-cli/src/component.rs` (`OpenMatch`, `MatchState`,
`rally::winner`, lines 16-19 and 218) stay with `play`.

## Acceptance

- `rally_split_matches_one_process`: rally run as three `run --component` processes, driven through
  the table's ingress, reaches the same `MatchState` after the same commands and ticks as `run` in
  one process.
- `a_second_specification_runs_split`: a two-component neutral fixture with one network-reached
  component, run as two processes, carries its event binding over TCP and the receiving component
  runs the bound command.
- `a_missing_peer_is_named`: a component whose events lead to a peer with no
  `SPINDLE_PEER_<COMPONENT>` exits 1 naming the peer and the variable, before anything runs.
- `play --component` keeps its behaviour and tests.

## Scope

- `crates/spindle-cli/src/component.rs` (the link set-up, `linked` line 111, `ingress` line 236,
  `peer_addresses` line 290, split from the rally loop).
- `crates/spindle-cli/src/cli.rs` (`Run`: `--component`, `--listen`), `crates/spindle-cli/src/main.rs`.
- `crates/spindle-host/src/host/component.rs` (`Host::component` line 440, read).
- `crates/spindle-cli/tests/fixtures/` (the second specification).

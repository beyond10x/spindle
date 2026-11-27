---
format: aep.planning-md/3
id: story:process-tests-hold-their-ports
kind: story
status: draft
title: The process tests hold the ports they hand a child
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

The process tests in `crates/spindle-cli/tests/spindle-cli/processes.rs` and
`adversary_processes2.rs` hold the ports they hand to a child until the child binds them, as
`adversary_processes.rs` does since wave 30, so no other process can take a port between its
choice and the child's bind.

## Why

Found by wave 30's U8 adversary (head 48e150dc): `free_addresses` at `processes.rs:60` and
`adversary_processes2.rs:45` drop the port before the child binds it, the race that made the
slow-peer connect test fail under load before wave 30; a self-connection there now fails too.

## Acceptance

- Both files take their ports the way `adversary_processes.rs` does (a loopback host of the case's
  own on Linux), and their tests pass 20 runs in a row at a 1-minute load of at least 40.

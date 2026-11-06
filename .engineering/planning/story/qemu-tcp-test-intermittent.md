---
format: aep.planning-md/3
id: story:qemu-tcp-test-intermittent
kind: story
status: draft
title: Two QEMU instances reach each other over TCP every time
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-qemu/tests/spindle-qemu/microvm.rs
revision: 2
---
# Two QEMU instances reach each other over TCP every time

## Outcome

`microvm::two_qemu_instances_reach_each_other_over_tcp` passes on every run of `task check:vm`,
or its failure names the cause. On 2026-11-06 it failed once in the wave 11 gate (the calling
microVM did not exit OK, `crates/spindle-qemu/tests/spindle-qemu/microvm.rs:295`), then passed
three times alone, once in the whole binary at four threads and once at one thread. The gate log
kept the panic line but not the two consoles, so the cause is not known.

## Acceptance

- `the_tcp_pair_passes_twenty_runs_in_a_row`: twenty runs of the whole spindle-qemu binary with
  `--include-ignored` pass, on the gate's thread count.
- A failure of the test prints both consoles and the time each microVM took to boot and to exit,
  so the next failure names its cause.

## Origin

Wave 11 gate, 2026-11-06: one failure in six runs; the hypothesis that more microVMs start at
once now that the crate has one test binary is not verified.

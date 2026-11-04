---
format: aep.planning-md/3
id: story:firecracker-launcher
kind: story
status: active
title: A Firecracker launcher
relations:
- serves: vision:spindle
- decomposes: epic:deployment-targets
- depends_on: story:qemu-microvm-launcher
scope:
- confidence: cited
  path: crates/spindle-firecracker
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T09:17:19Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-04T09:17:19Z", actor: "human:timo", revision: 5}
---
# A Firecracker launcher

## Outcome

A `firecracker` launcher implements the target contract with the Firecracker release binary,
without root: the instances' taps and bridge live in a user and network namespace, each VM gets
the requested vCPUs and memory through its machine configuration, and the console and exit are
reported as for QEMU.

## Acceptance

- `a_firecracker_instance_boots_and_runs_b10x_spindle` — skipped with its reason where
  `/dev/kvm` or the binary is absent.
- `two_firecracker_instances_reach_each_other_over_tcp`.
- `the_same_rally_placement_runs_on_qemu_and_on_firecracker` — only the target kind differs.

## Scope

`crates/spindle-firecracker/` (new).

## Open

Whether Firecracker opens taps inside a user namespace (untested on this host).

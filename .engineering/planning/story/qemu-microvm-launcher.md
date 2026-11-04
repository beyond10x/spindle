---
format: aep.planning-md/3
id: story:qemu-microvm-launcher
kind: story
status: implemented
title: A QEMU microVM launcher
relations:
- serves: vision:spindle
- decomposes: epic:deployment-targets
- depends_on: story:deployment-target-contract
scope:
- confidence: cited
  path: crates/spindle-qemu
- confidence: cited
  path: examples/rally/vm
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T09:17:18Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-04T09:17:19Z", actor: "human:timo", revision: 5}
- {from: "active", to: "implemented", at: "2026-11-04T10:52:47Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":1}}}
---
# A QEMU microVM launcher

## Outcome

A `qemu-microvm` launcher implements the target contract with `qemu-system-x86_64 -M microvm`
and KVM, without root: it builds an initramfs holding a static `b10x-spindle` and an init that
sets the guest's address, boots it with the requested vCPUs and memory, joins the instances on one
QEMU socket network, captures the console, and reports the exit.

## Acceptance

- `a_qemu_microvm_instance_boots_and_runs_b10x_spindle` — skipped with its reason where
  `/dev/kvm` or QEMU is absent.
- `two_qemu_instances_reach_each_other_over_tcp`.
- `vcpus_and_memory_are_passed_to_the_vm`.

## Scope

`crates/spindle-qemu/` (new; implements the `Launcher` port from `spindle-target` once that crate is on the integration branch), `examples/rally/vm/`.

## Open

The guest kernel: download a published one with a checksum, or build one.

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
revision: 8
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

All three are in `crates/spindle-qemu/tests/microvm.rs` and run under `task check:vm`.

## Scope

- `crates/spindle-qemu/` — cited.

`examples/rally/vm/` was in the first scope and was never created: the initramfs, the static build
and the guest init live in `crates/spindle-qemu/src/` (`cpio.rs`, `static_build.rs`,
`bin/init.rs`). Taken out of the typed scope by a plan review on 2026-11-04.

## Open

The guest kernel: the launcher boots the host kernel (`crates/spindle-qemu/src/kernel.rs`); a
published kernel with a checksum was not needed.

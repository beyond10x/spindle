---
title: Deployment targets
sidebar_position: 5
description: Where an instance of a component runs - a host process, a QEMU microvm or a Firecracker microVM - and the launcher contract they share.
lede: Every target kind implements one launcher contract, so the rest of Spindle never knows whether an instance is a process or a virtual machine.
source: ess/domains/placement.yaml, crates/spindle-target/src/lib.rs, crates/spindle-target/src/placement.rs, crates/spindle-target/tests/spindle-target/target.rs, crates/spindle-qemu/src/lib.rs, crates/spindle-firecracker/src/lib.rs, AGENTS.md (Gate), docs/design/spindle.md §12, docs/design/observe.md
---

Spindle's own specification has a domain for this decision, `spindle.placement`
(`ess/domains/placement.yaml`). A **target** is a place an instance can be launched, with the
environment it provides and the most resources one instance may take there. An **instance** is
one component placed on one target.

## Placement

Placement refuses an instance when its target is retired, when the component's admission was
decided for another environment than the target's, when the target's environment does not admit
the component, or when the instance asks for more than the target's ceiling. The refusal
lists every reason that applies.

## The launcher contract

A placed instance is handed to a **launcher** as a launch plan: its identity, its component, its
settings, the artifact to run, the resources it asked for, where it listens and where its peers
listen. The launcher starts it, stops it and reports its exit. A launcher refuses, before anything
starts, a request for a resource it cannot enforce.

## Target kinds

| Target | Crate | State |
|---|---|---|
| Process | `spindle-target` | a child process on the host; the reference launcher. It enforces no resource, so it refuses a plan that asks for one. |
| QEMU microvm | `spindle-qemu` | one `qemu-system-x86_64 -M microvm` per instance with KVM, without root. The guest is the host's kernel and an initramfs with a static `b10x-spindle` and a Rust `/init`. vCPUs and memory are enforced, disk is not. All VMs of one launcher share a socket network inside a user and network namespace the launcher owns. |
| Firecracker | `spindle-firecracker` | the Firecracker release binary inside a user and network namespace, one tap per instance on a bridge, without root. vCPUs and memory are set through the API socket; disk is not enforced. |

The specification also lists Docker, Compose, Kubernetes and browser targets. No launcher exists
for them.

The QEMU and Firecracker launchers are libraries: no `b10x-spindle` command starts a VM yet. Their
tests need `/dev/kvm` and are ignored in CI. `task check:vm` runs them on a host that has it.
Firecracker also needs the Firecracker binary and a guest kernel.

Each launcher also samples, from `/proc`, the CPU seconds and resident memory an instance costs
the host and its share of the request ([observe](/docs/concepts/observe)).

:::caution[Planned]
A bundle that is uploaded to deploy, and cells with ownership leases, are planned. The design is in
[`docs/design/spindle.md`](https://github.com/beyond10x/spindle/blob/main/docs/design/spindle.md)
§12. See [status](/docs/status).
:::

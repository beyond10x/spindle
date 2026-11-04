---
format: aep.planning-md/3
id: story:rally-runs-in-three-microvms
kind: story
status: draft
title: Rally runs in three microVMs
relations:
- decomposes: epic:m1-m3-rally-in-three-vms
- depends_on: story:qemu-microvm-launcher
revision: 2
---
# Rally runs in three microVMs

## Outcome

Each rally component runs in its own microVM, one `b10x-spindle run` per VM, joined by TCP
channels across the VM network, and the match ends with a winner.

## Acceptance

- `a_match_between_three_microvms_ends_with_a_winner` — on a host with KVM; skipped with its
  reason where `/dev/kvm` is absent.

## Scope

`examples/rally/vm/` (new).

## Host findings (scout, 2026-11-04)

- `/dev/kvm`, `/dev/net/tun` and `/dev/vhost-vsock` are mode 0666; no passwordless sudo.
- Not installed: `firecracker`, `jailer`, `cloud-hypervisor`. Installed: QEMU 11.1.2 with the
  `microvm` machine type, `passt`/`pasta`, `unshare`, the `x86_64-unknown-linux-musl` target.
- The `mantle` repository has no local microVM launcher; the `substrate` Firecracker driver is
  `proposed` (`substrate/.engineering/planning/epic/firecracker-driver.md`).
- vsock connects guest to host only, so guest-to-guest TCP needs a shared VM network.

## Path

1. Static musl build of `b10x-spindle`.
2. A guest kernel with virtio-mmio, virtio-net built in (the host kernel has them as modules).
3. One initramfs per VM: init sets a static IP and execs `b10x-spindle run`.
4. No-root route: three `qemu-system-x86_64 -M microvm` processes on one `-netdev socket`
   network. Firecracker route: three VMs with taps and a bridge inside `unshare -rn` (untested
   whether Firecracker opens taps in a user namespace).

## Open

QEMU `microvm` first or Firecracker inside a user namespace.

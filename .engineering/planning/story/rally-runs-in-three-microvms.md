---
format: aep.planning-md/3
id: story:rally-runs-in-three-microvms
kind: story
status: draft
title: Rally runs in three microVMs
relations:
- decomposes: epic:m1-m3-rally-in-three-vms
revision: 1
---
# Rally runs in three microVMs

## Outcome

Each rally component runs in its own Firecracker microVM, one `b10x-spindle run` per VM, joined by
TCP channels across the VM network, and the match ends with a winner.

## Acceptance

- `a_match_between_three_microvms_ends_with_a_winner` — on a host with KVM; skipped with its
  reason where `/dev/kvm` is absent.

## Scope

`examples/rally/vm/` (new).

## Open

Whether the VMs come from `mantle` or a substrate Firecracker driver; neither exists for this yet.

---
format: aep.planning-md/3
id: story:rally-runs-in-three-microvms
kind: story
status: implemented
title: Rally runs in three microVMs
relations:
- decomposes: epic:m1-m3-rally-in-three-vms
- depends_on: story:qemu-microvm-launcher
- serves: vision:spindle
- depends_on: story:rally-runs-as-three-processes
- depends_on: story:firecracker-launcher
- depends_on: story:telemetry-transport
scope:
- confidence: cited
  path: crates/spindle-firecracker
- confidence: cited
  path: crates/spindle-qemu
- confidence: inferred
  path: examples/rally
revision: 9
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T21:33:53Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"review_outcome":1}}}
- {from: "proposed", to: "active", at: "2026-11-04T21:33:53Z", actor: "human:timo", revision: 8, decided_on: {"recorded":{"review_outcome":1}}}
- {from: "active", to: "implemented", at: "2026-11-05T22:03:28Z", actor: "human:timo", revision: 9, decided_on: {"recorded":{"test_result":2,"review_outcome":1}}}
---
# Rally runs in three microVMs

## Outcome

Each rally component runs in its own microVM, one `b10x-spindle` per VM in its per-component
mode, joined by TCP channels across the VM network, and the match ends with a winner (design
§ 12, M3). The same placement runs on the QEMU and on the Firecracker target; only the target
kind differs.

## Acceptance

Each is an `#[ignore]`d VM test run by `task check:vm`, skipped with its reason where a
prerequisite (`/dev/kvm`, QEMU, the Firecracker binary or kernel) is absent.

- `a_match_between_three_qemu_microvms_ends_with_a_winner` — the table's `MatchState.winner` is
  set; read through the console or the view.
- `a_match_between_three_firecracker_microvms_ends_with_a_winner` — the same on Firecracker.
- `the_same_rally_placement_runs_on_qemu_and_on_firecracker` — one placement document, two
  target kinds, both end with a winner.

## Gap carried from `story:firecracker-launcher`

`story:firecracker-launcher` is implemented and names
`the_same_rally_placement_runs_on_qemu_and_on_firecracker` in its acceptance, but no test of that
name exists on `main` at `b9b0cd9` (`crates/spindle-firecracker/tests/`, `crates/spindle-qemu/tests/`).
It needs rally to run one component per instance, which `story:rally-runs-as-three-processes`
builds, so it is owed here. Recorded by a plan review on 2026-11-04.

## Scope

- `crates/spindle-qemu` — cited: the VM tests live in `tests/`; the guest init must start the
  per-component mode.
- `crates/spindle-firecracker` — cited: the same, `src/bin/spindle-guest-init.rs`.
- `examples/rally` — inferred: the placement document the cross-target test reads.

The first scope, `examples/rally/vm/`, does not exist: the initramfs, the static build and the
guest init live in the launcher crates.

## Depends on

`story:rally-runs-as-three-processes` (added by the plan review), `story:qemu-microvm-launcher`,
`story:firecracker-launcher` (added by the plan review), and `story:telemetry-transport` (added
after the round-1 critics, 2026-11-04): both stories edit the guest init binaries
(`crates/spindle-qemu/src/bin/init.rs`, `crates/spindle-firecracker/src/bin/spindle-guest-init.rs`);
transport adds the vsock endpoint first, this story then starts the per-component mode from the
same init.

## Host findings (scout, 2026-11-04)

Superseded by the launchers: the QEMU launcher boots the host kernel with the virtio modules
loaded by its init (`crates/spindle-qemu/src/kernel.rs`), joins guests on one QEMU socket network,
and the Firecracker launcher runs taps and a bridge inside a user namespace. The scout's "Path"
and its open question (QEMU first or Firecracker) are answered: both exist.

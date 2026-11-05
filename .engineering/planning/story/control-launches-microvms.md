---
format: aep.planning-md/3
id: story:control-launches-microvms
kind: story
status: draft
title: The control process launches QEMU and Firecracker instances
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-control/src/control.rs
- confidence: cited
  path: crates/spindle-control/src/lib.rs
- confidence: inferred
  path: crates/spindle-firecracker
- confidence: inferred
  path: crates/spindle-qemu
revision: 3
---
# The control process launches QEMU and Firecracker instances

## Outcome

`b10x-spindle deploy` launches instances placed on `QemuMicrovm` and Firecracker targets through
their launchers and collects their telemetry, as it does for `Process` targets. Today
`crates/spindle-control/src/control.rs` (`Control::start`) launches `Process` targets only and lists
every other instance as not run (`Control::not_run`).

## Acceptance

- `a_qemu_instance_is_launched_by_control` — a placement with one instance on a QEMU target: the
  instance runs, streams samples and is not listed as not run (VM test, `task check:vm`).
- `a_firecracker_instance_is_launched_by_control` — the same on Firecracker.

## Origin

Cut from `story:control-process-serves-the-api` (wave 6).

## Scope

- `crates/spindle-control/src/control.rs` — cited: the launch path and `not_run`.
- `crates/spindle-control/src/lib.rs` — cited: the launch section of the crate docs.
- `crates/spindle-qemu` — inferred: its launcher.
- `crates/spindle-firecracker` — inferred: its launcher.

---
format: aep.planning-md/3
id: story:telemetry-binding-per-vm-instance
kind: story
status: draft
title: Telemetry from VM and vsock instances is bound to the right instance
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-control/src/control.rs
- confidence: inferred
  path: crates/spindle-control/tests/adversary_control.rs
- confidence: inferred
  path: crates/spindle-telemetry/src/vsock.rs
revision: 3
---
# Telemetry from VM and vsock instances is bound to the right instance

## Outcome

A telemetry frame from a VM or vsock instance counts only for the instance that sent it, and the
peer check covers every target kind. Today a connection is bound to the instance id of its first
frame, and the process-group peer check covers process targets only
(`crates/spindle-control/src/control.rs`, `Supervisor::frame`), so a VM or vsock instance can
report as another.

## Acceptance

- `a_vm_frame_naming_another_instance_is_dropped` — over a vsock connection, a frame naming an
  instance other than the one launched there is dropped and not streamed.
- `the_peer_check_covers_vm_targets` — the binding of a connection to its instance comes from the
  launcher, not from the first frame.

## Origin

From the adversary on `story:control-process-serves-the-api`, finding F3 (wave 6).

## Scope

- `crates/spindle-control/src/control.rs` — cited: the connection binding and `Supervisor::frame`.
- `crates/spindle-telemetry/src/vsock.rs` — inferred.
- `crates/spindle-control/tests/adversary_control.rs` — inferred.

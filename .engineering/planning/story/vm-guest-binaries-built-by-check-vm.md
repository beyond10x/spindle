---
format: aep.planning-md/3
id: story:vm-guest-binaries-built-by-check-vm
kind: story
status: draft
title: task check:vm builds the guest binaries once
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: Taskfile.yml
- confidence: inferred
  path: crates/spindle-firecracker/tests/firecracker.rs
- confidence: cited
  path: crates/spindle-firecracker/tests/rally.rs
- confidence: inferred
  path: crates/spindle-qemu/src/static_build.rs
- confidence: inferred
  path: crates/spindle-qemu/tests/support/rally.rs
revision: 3
---
# task check:vm builds the guest binaries once

## Outcome

`task check:vm` builds the static musl guest binaries once, before the VM tests, and locates the
Firecracker binary and kernel; the VM tests require those binaries and no longer run a nested
`cargo build` inside the test run. Today the rally VM tests build them themselves
(`crates/spindle-firecracker/tests/rally.rs`, `$CARGO build --release --target
x86_64-unknown-linux-musl`), and `check:vm` in `Taskfile.yml` only runs `cargo test`.

## Acceptance

- `check:vm` builds the guest binaries in one step before `cargo test` and exports where they and
  the Firecracker binary and kernel are.
- `a_vm_test_without_the_guest_binaries_names_them` — a VM test run without the binaries fails
  naming the missing path and the task that builds it, without starting `cargo`.

## Origin

Found in wave 6 while running `task check:vm` for `story:rally-runs-in-three-microvms`.

## Scope

- `Taskfile.yml` — cited: `check:vm`.
- `crates/spindle-firecracker/tests/rally.rs` — cited: the nested build.
- `crates/spindle-firecracker/tests/firecracker.rs` — inferred.
- `crates/spindle-qemu/tests/support/rally.rs` — inferred.
- `crates/spindle-qemu/src/static_build.rs` — inferred.

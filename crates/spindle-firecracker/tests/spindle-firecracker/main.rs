//! The integration tests of `spindle-firecracker`, one module per former `tests/*.rs` file, linked into
//! one test binary (`story:one-test-binary-per-crate`). Test names are unchanged under their
//! module path.
//!
//! The VM modules are Linux only: they need KVM, user and network namespaces and vsock.
//! `off_linux` is the rest: there the launcher and the guest init refuse
//! (`story:macos-host-builds`).

#[cfg(target_os = "linux")]
#[path = "../../../spindle-qemu/tests/support/rally.rs"]
mod rally_support;

#[cfg(target_os = "linux")]
mod adversary;
#[cfg(target_os = "linux")]
mod adversary_rally;
#[cfg(target_os = "linux")]
mod firecracker;
#[cfg(not(target_os = "linux"))]
mod off_linux;
#[cfg(target_os = "linux")]
mod rally;

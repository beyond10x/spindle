//! The integration tests of `spindle-qemu`, one module per former `tests/*.rs` file, linked into
//! one test binary (`story:one-test-binary-per-crate`). Test names are unchanged under their
//! module path.
//!
//! `tests/context_ids.rs` stays its own binary: it takes every guest context id the process may
//! give out, which the VM cases here also draw from.
//!
//! The VM modules are Linux only: they need KVM, user and network namespaces and vsock.
//! `off_linux` is the rest: there the launcher and the guest init refuse
//! (`story:macos-host-builds`).

#[cfg(target_os = "linux")]
#[path = "../support/rally.rs"]
mod rally_support;

#[cfg(target_os = "linux")]
mod adversary;
#[cfg(target_os = "linux")]
mod adversary_rally;
#[cfg(target_os = "linux")]
mod adversary_transport;
#[cfg(target_os = "linux")]
mod initramfs;
#[cfg(target_os = "linux")]
mod microvm;
#[cfg(not(target_os = "linux"))]
mod off_linux;
#[cfg(target_os = "linux")]
mod rally;

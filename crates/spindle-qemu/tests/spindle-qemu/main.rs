//! The integration tests of `spindle-qemu`, one module per former `tests/*.rs` file, linked into
//! one test binary (`story:one-test-binary-per-crate`). Test names are unchanged under their
//! module path.
//!
//! `tests/context_ids.rs` stays its own binary: it takes every guest context id the process may
//! give out, which the VM cases here also draw from.

#[path = "../support/rally.rs"]
mod rally_support;

mod adversary;
mod adversary_rally;
mod adversary_transport;
mod initramfs;
mod microvm;
mod rally;

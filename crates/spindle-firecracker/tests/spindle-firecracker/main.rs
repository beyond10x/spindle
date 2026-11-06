//! The integration tests of `spindle-firecracker`, one module per former `tests/*.rs` file, linked into
//! one test binary (`story:one-test-binary-per-crate`). Test names are unchanged under their
//! module path.

#[path = "../../../spindle-qemu/tests/support/rally.rs"]
mod rally_support;

mod adversary;
mod adversary_rally;
mod firecracker;
mod rally;

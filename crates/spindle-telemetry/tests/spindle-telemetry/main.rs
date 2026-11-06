//! The integration tests of `spindle-telemetry`, one module per former `tests/*.rs` file, linked into
//! one test binary (`story:one-test-binary-per-crate`). Test names are unchanged under their
//! module path.
//!
//! `tests/blocking.rs` stays its own binary: it installs a counting global allocator, which
//! would replace the allocator of every test here.

mod adversary_deletes;
mod adversary_record;
mod adversary_transport;
mod catalogue;
mod record;
mod transport;

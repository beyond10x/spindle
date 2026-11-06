//! The integration tests of `spindle-target`, one module per former `tests/*.rs` file, linked into
//! one test binary (`story:one-test-binary-per-crate`). Test names are unchanged under their
//! module path.

mod adversary;
mod adversary_sampling;
mod sampling;
mod target;
mod telemetry;

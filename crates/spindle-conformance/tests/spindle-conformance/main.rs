//! The integration tests of `spindle-conformance`, one module per former `tests/*.rs` file, linked into
//! one test binary (`story:one-test-binary-per-crate`). Test names are unchanged under their
//! module path.

mod adversary_caller;
mod adversary_conform;
mod adversary_conversions;
mod adversary_deletes;
mod adversary_establish;
mod adversary_grants;
mod adversary_refusal;
mod adversary_retry;
mod adversary_w17_conformance;
mod aggregates;
mod conversions;
mod establish;
mod forcing;
mod gate;
mod grants;
mod refusal;
mod retry;

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
mod adversary_w29_u2;
mod aggregates;
mod before_row_and_reference_lists;
mod computed;
mod conversions;
mod establish;
mod explore;
mod forcing;
mod gate;
mod grants;
mod held_null;
mod payload_conversions;
mod refusal;
mod retry;
mod supplied;
mod view_paging;

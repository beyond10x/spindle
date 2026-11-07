//! The integration tests of `spindle-kernel`, one module per former `tests/*.rs` file, linked into
//! one test binary (`story:one-test-binary-per-crate`). Test names are unchanged under their
//! module path.

mod rally_cases;

mod adversary;
mod adversary_admission;
mod adversary_append;
mod adversary_boolean;
mod adversary_bytes;
mod adversary_deletes;
mod adversary_generated;
mod adversary_mint_text;
mod adversary_placement;
mod adversary_rally;
mod adversary_related_guards;
mod adversary_related_values;
mod adversary_responses;
mod adversary_store_contract;
mod adversary_structured_responses;
mod caller;
mod deletes;
mod deployment;
mod error_fields;
mod generated;
mod guards;
mod list_append;
mod mint;
mod placement;
mod rally;
mod related_guards;
mod related_values;
mod responses;
mod row_set_guards;
mod scan;
mod store_contract;
mod structured_responses;
mod unsupported;

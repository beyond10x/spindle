//! The integration tests of `spindle-kernel`, one module per former `tests/*.rs` file, linked into
//! one test binary (`story:one-test-binary-per-crate`). Test names are unchanged under their
//! module path.

mod rally_cases;

mod adversary;
mod adversary_admission;
mod adversary_admission_checked;
mod adversary_append;
mod adversary_boolean;
mod adversary_bytes;
mod adversary_creation_unknown;
mod adversary_deletes;
mod adversary_environment;
mod adversary_generated;
mod adversary_launch_admission;
mod adversary_launch_refused;
mod adversary_look_ahead;
mod adversary_mint_text;
mod adversary_placement;
mod adversary_rally;
mod adversary_related_guards;
mod adversary_related_values;
mod adversary_responses;
mod adversary_row_set;
mod adversary_scan_version;
mod adversary_set_outcomes;
mod adversary_store_contract;
mod adversary_structured_responses;
mod caller;
mod creation_takes_unknown_row;
mod deletes;
mod deployment;
mod error_fields;
mod generated;
mod guard_reads_now;
mod guards;
mod list_append;
mod mint;
mod placement;
mod provides;
mod rally;
mod related_guards;
mod related_values;
mod responses;
mod row_set_guards;
mod scan;
mod set_outcomes;
mod store_contract;
mod structured_responses;
mod unsupported;

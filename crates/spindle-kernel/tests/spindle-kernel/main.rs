//! The integration tests of `spindle-kernel`, one module per former `tests/*.rs` file, linked into
//! one test binary (`story:one-test-binary-per-crate`). Test names are unchanged under their
//! module path.

mod rally_cases;

mod absent_optional;
mod absent_optional_guard;
mod adversary;
mod adversary_admission;
mod adversary_admission_checked;
mod adversary_append;
mod adversary_boolean;
mod adversary_bytes;
// `adversary_counts` moved to the unit tests of `src/sqlite.rs`, held in calls of the event
// store instead of time (story:ci-tests-deterministic).
mod adversary_creation_unknown;
mod adversary_deletes;
mod adversary_environment;
mod adversary_existing_instance;
mod adversary_expressions;
mod adversary_generated;
mod adversary_guard_now;
mod adversary_launch_admission;
mod adversary_launch_refused;
mod adversary_look_ahead;
mod adversary_mint_text;
mod adversary_placement;
#[cfg(feature = "postgres")]
mod adversary_postgres;
#[cfg(all(feature = "postgres", feature = "sqlite"))]
mod adversary_postgres_named;
#[cfg(feature = "postgres")]
mod adversary_postgres_scan;
mod adversary_rally;
mod adversary_related_guards;
mod adversary_related_values;
mod adversary_responses;
mod adversary_row_set;
mod adversary_scan_version;
mod adversary_secondary_effects;
mod adversary_set_outcomes;
mod adversary_state_move;
mod adversary_store_contract;
mod adversary_structured_responses;
#[cfg(feature = "postgres")]
mod adversary_w21_store;
mod adversary_w25_guard;
mod adversary_w25_mint;
mod before_row;
mod caller;
mod creation_takes_unknown_row;
mod deletes;
mod deployment;
mod error_fields;
mod existing_instance;
mod expressions;
mod generated;
mod guard_reads_now;
mod guards;
mod held_null;
mod list_append;
mod mint;
mod placement;
#[cfg(feature = "postgres")]
mod postgres;
#[cfg(all(unix, feature = "sqlite"))]
mod private_state;
mod provides;
mod rally;
mod reference_list;
mod related_guards;
mod related_values;
mod response_source;
mod responses;
mod resting_rows;
mod review_undecidable_disclosure;
mod row_set_guards;
mod scan;
mod secondary_effects;
mod set_outcomes;
mod state_change;
#[cfg(all(feature = "postgres", feature = "sqlite"))]
mod state_move;
mod store_contract;
mod structured_responses;
mod unsupported;
mod upsert_creation_related_refusal;
mod where_selected_values;

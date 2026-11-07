//! The integration tests of `spindle-cli`, one module per former `tests/*.rs` file, linked into
//! one test binary (`story:one-test-binary-per-crate`). Test names are unchanged under their
//! module path.

mod admit;
mod adversary_caller;
mod adversary_console;
mod adversary_control;
mod adversary_delete;
mod adversary_fixed_caller;
mod adversary_form_query;
mod adversary_ingress;
mod adversary_processes;
mod adversary_processes2;
mod adversary_replay;
mod adversary_responses;
mod adversary_served_decisions;
mod adversary_unserved;
mod adversary_unserved_base;
mod adversary_world;
mod caller;
mod conform;
mod console;
mod decisions;
mod deletes;
mod deploy;
mod fixed_caller;
mod fixed_callers;
mod form_query;
mod ingress;
mod metrics;
mod mint;
mod outcome_scoped_refusal;
mod play;
mod processes;
mod replay;
mod responses;
mod run;
mod served_decisions;
mod served_world;
mod unserved;

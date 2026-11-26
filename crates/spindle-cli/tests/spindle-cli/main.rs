//! The integration tests of `spindle-cli`, one module per former `tests/*.rs` file, linked into
//! one test binary (`story:one-test-binary-per-crate`). Test names are unchanged under their
//! module path.

mod absent_optional_guard;
mod admit;
mod adversary2_console_paths;
mod adversary2_http_cause;
mod adversary_binding_where;
mod adversary_caller;
mod adversary_console;
mod adversary_control;
mod adversary_delete;
mod adversary_external_answers;
mod adversary_fixed_caller;
mod adversary_form_query;
mod adversary_http_cause;
mod adversary_ingress;
mod adversary_outcome_bodies;
mod adversary_outcome_scoped_refusal;
mod adversary_postgres;
mod adversary_processes;
mod adversary_processes2;
mod adversary_prometheus;
mod adversary_replay;
mod adversary_responses;
mod adversary_served_decisions;
mod adversary_state_move;
mod adversary_topology;
mod adversary_topology_emitted;
mod adversary_trusted_caller;
mod adversary_unserved;
mod adversary_unserved_base;
mod adversary_view_list_parameter;
mod adversary_w17_conform;
mod adversary_w21_state_lock;
mod adversary_w25_mint;
mod adversary_w25_mint_r2;
mod adversary_w26_u3;
mod adversary_world;
mod adversary_world_spec;
mod binding_shapes;
mod binding_where;
mod caller;
mod clock_and_mint;
mod concurrent_reads;
mod conform;
mod console;
mod decisions;
mod deletes;
mod deploy;
mod error_object_bodies;
mod fixed_caller;
mod fixed_callers;
mod form_query;
mod format_range;
mod http_cause;
mod ingress;
mod metrics;
mod mint;
mod observe;
mod observe_panel;
mod outcome_bodies;
mod outcome_scoped_refusal;
mod play;
mod postgres;
mod processes;
mod rejected_line;
mod replay;
mod response_source;
mod responses;
mod route_precedence;
mod run;
mod scenario_reads;
mod seed;
mod served_decisions;
mod served_operations;
mod served_world;
mod state_lock;
mod state_move;
mod tls_cause;
mod topology;
mod trusted_caller;
mod unserved;
mod view_field_filters;
mod view_list_parameter;

/// The PostgreSQL advisory lock a case holds while it runs `GRANT … ON DATABASE`: cases granting
/// on the one test database at once fail with "tuple concurrently updated".
const DATABASE_GRANT_LOCK: i64 = 5_312_021_917;

/// The statement that leaves `role` only `SELECT` on the migration metadata of every component
/// in `schema` (`<prefix>_schema_version`, `<prefix>_projection_registry`,
/// `<prefix>_settlement_floor`): eventlog's hosted open refuses a role that can write them.
fn metadata_read_only(schema: &str, role: &str) -> String {
    format!(
        "DO $$ DECLARE t text; BEGIN FOR t IN SELECT format('%I.%I', schemaname, tablename) FROM \
         pg_tables WHERE schemaname = '{schema}' AND (tablename LIKE '%\\_schema\\_version' OR \
         tablename LIKE '%\\_projection\\_registry' OR tablename LIKE '%\\_settlement\\_floor') \
         LOOP EXECUTE 'REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON ' || t || ' FROM {role}'; END \
         LOOP; END $$"
    )
}

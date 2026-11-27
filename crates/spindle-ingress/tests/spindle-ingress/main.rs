//! The integration tests of `spindle-ingress`, one module per former `tests/*.rs` file, linked into
//! one test binary (`story:one-test-binary-per-crate`). Test names are unchanged under their
//! module path.

mod adversary2_caller_proof;
mod adversary_component_reset;
mod adversary_delivery;
mod adversary_refusal_payload;
mod adversary_route_precedence;
mod adversary_router;
mod adversary_unpublished_event;
mod adversary_view_parameters;
mod adversary_w21_security;
mod adversary_w22_exposed;
mod adversary_w27_u3;
mod adversary_w27_u7;
mod adversary_w28_u3;
mod always_and_key_routes;
mod binding_shapes;
mod close;
mod concurrent_reads;
mod ess_http_shapes;
mod events_from_store;
mod events_range;
mod exposed;
mod grants;
mod metrics_read;
mod observe;
mod outcome_headers;
mod private_record;
mod query_beside_body;
mod refusal_payload;
mod router;
mod served;
mod topology;
mod unpublished_event;
mod view_paging;
mod view_parameters;
mod view_wire_names;

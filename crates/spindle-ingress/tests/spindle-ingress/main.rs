//! The integration tests of `spindle-ingress`, one module per former `tests/*.rs` file, linked into
//! one test binary (`story:one-test-binary-per-crate`). Test names are unchanged under their
//! module path.

mod adversary_component_reset;
mod adversary_delivery;
mod adversary_refusal_payload;
mod adversary_route_precedence;
mod adversary_router;
mod adversary_view_parameters;
mod observe;
mod refusal_payload;
mod router;
mod view_parameters;

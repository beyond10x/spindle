//! The HTTP adapter behind the `ingress.network` capability, bound to `http-loopback`.
//!
//! A component that requires `ingress.network` is served on 127.0.0.1. What it serves is what
//! ESS derives for it: each command it accepts and, where it is `reached_by: network`, each view
//! of a domain it owns. Where the routes are comes from ESS too:
//!
//! - with an `ess-http/1` binding, compiled by ESS ([`ess_http::compile`],
//!   [`ess_gen::http::Binding`]), each operation is at the method and path
//!   [`ess_gen::http::bound_routes`] gives it, path variables read into the input as
//!   [`ess_gen::http::scalar`] says, and each outcome answers the status the binding lists or ESS
//!   derives;
//! - without one, the fallback: `POST /api/commands/<qualified name>` and
//!   `GET /api/views/<qualified name>`, each outcome at ESS's derived status
//!   ([`ess_http::status::outcome_status`]).
//!
//! A command's body is the input ESS projects; it answers `{outcome, error?, events,
//! invocations}` ([`answer`]). A view answers the body ESS projects, `{"rows": [...]}`.
//!
//! [`Listener::bind`] owns the socket and the HTTP server on a thread of its own; every request
//! for a route of the [`Surface`] arrives as an [`Exchange`] on [`Listener::next`], so the
//! [`spindle_host::Host`] that answers it stays on the caller's thread.

mod answer;
mod listener;
mod surface;

pub use answer::{Answer, answer};
pub use listener::{BODY_LIMIT, Exchange, Listener};
pub use surface::{Resolved, Route, Surface, SurfaceError, Variable, fallback};

/// The `ingress.network` implementation this crate serves: HTTP on the loopback address.
pub const HTTP_LOOPBACK: &str = "http-loopback";

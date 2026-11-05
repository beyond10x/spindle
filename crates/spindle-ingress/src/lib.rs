//! The HTTP adapter behind the `ingress.network` capability, bound to `http-loopback`.
//!
//! A component that requires `ingress.network` is served on 127.0.0.1, exactly as ESS publishes
//! it. What it serves is what ESS derives for it: each command it accepts and, where it is
//! `reached_by: network`, each view of a domain it owns. Where each is served is where
//! [`ess_gen::http::bound_routes`] puts it, which is where `ess generate --kind openapi`
//! publishes it: without a binding ESS's derived routes (`/<domain>/commands/<command>`,
//! `/<domain>/views/<view>`); with an `ess-http/1` binding, compiled by ESS
//! ([`ess_http::compile`], [`ess_gen::http::Binding`]), the binding's method, path, path
//! variables and status per outcome for the operations it names, and ESS's derived routes for
//! the rest.
//!
//! A command's body is the input ESS publishes; it answers the envelope ESS publishes for the
//! outcome, `{outcome, published, error?, payload?}` ([`answer`]). A view answers the body ESS
//! publishes, `{"rows": [...]}`.
//!
//! [`Listener::bind`] owns the socket and a hyper HTTP/1.1 server on a thread of its own; every
//! request for a route of the [`Surface`] arrives as an [`Exchange`] on [`Listener::next`], so
//! the [`spindle_host::Host`] that answers it stays on the caller's thread
//! ([`Exchange::serve`]).
//!
//! Beside those, a surface bound to `http-loopback` serves the routes a generic model console
//! reads ([`Surface::with_console`], [`Console`]): `GET /api/model`, the IR as
//! `ess specify compile --format json` prints it and each outcome's subject; `GET /api/events`,
//! every event published so far, in order; and `POST /api/reset`, which empties every store of the
//! run. A route the binding or ESS publishes at one of those paths is refused at startup.

mod answer;
pub mod console;
mod listener;
mod surface;

pub use answer::{Answer, answer};
pub use console::Console;
pub use listener::{BODY_LIMIT, BODY_READ, Exchange, HEADER_READ, Listener};
pub use surface::{Resolved, Route, Surface, SurfaceError, Variable};

/// The `ingress.network` implementation this crate serves: HTTP on the loopback address.
pub const HTTP_LOOPBACK: &str = "http-loopback";

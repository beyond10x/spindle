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
//! A command's request carries the input ESS publishes, as a JSON body or, where its binding
//! declares `request: {encoding: form}` or `{encoding: query}`, as a form body or the query
//! string; it answers the envelope ESS publishes for the
//! outcome, `{outcome, published, error?, payload?, response?}` ([`answer`]). A view answers the body ESS
//! publishes, `{"rows": [...]}`.
//!
//! A command or view this ingress cannot route, a command the kernel does not run, and a view the
//! host does not read, keeps its route and is [`Unserved`]: it answers `501` naming the operation
//! and the construct (with the envelope ESS publishes for an unfinished realization,
//! `{refused, committed: false}`), and [`Surface::unserved`] lists them so a run can name them
//! before it listens.
//!
//! The routing itself is transport-free: [`route`] answers a [`Request`] value on a
//! [`spindle_host::Host`] with a [`Response`] value, with no listener, socket or async runtime in
//! it, so a host with no socket, such as a Service Worker, answers the same routes the same way.
//! Without the default `listener` feature only that part is built, and it builds for
//! `wasm32-unknown-unknown`.
//!
//! `Listener::bind` (feature `listener`) owns the socket and a hyper HTTP/1.1 server on a thread
//! of its own; every request for a route of the [`Surface`] arrives as an `Exchange` on
//! `Listener::next`, so the [`spindle_host::Host`] that answers it stays on the caller's thread
//! (`Exchange::serve`, which answers it through the router).
//!
//! Beside those, a surface bound to `http-loopback` serves the routes a generic model console
//! reads ([`Surface::with_console`], [`Console`]): `GET /api/model`, the IR as
//! `ess specify compile --format json` prints it and each outcome's subject; `GET /api/events`,
//! every event published so far, in order; and `POST /api/reset`, which empties every store of the
//! run. A route the binding or ESS publishes at one of those paths is refused at startup.
//!
//! Paths under `/__spindle/` are the run's own ([`Reserved`]): [`Surface::resolve`] matches them
//! before any route, and a route the binding or ESS publishes there is refused at startup
//! ([`SurfaceError::Reserved`]). A surface made [`Surface::observing`] holds a [`Recorder`]: the
//! listener records every request it answers outside the prefix, classed by its answer
//! ([`Class`]) and timed from its head read to its answer handed back, and answers
//! `GET /__spindle/requests` with the request log, and `GET /__spindle/ui` with the observe
//! panel's page ([`Surface::with_panel`], [`PANEL_MEDIA`] with [`PANEL_POLICY`]), `503` where it
//! was given none; `GET /__spindle/metrics` answers the run's metrics as the Prometheus text
//! format ([`METRICS_MEDIA`]): the requests counted, the host's counters per component, the rows
//! each entity holds, the resets and the external causes answered, read on the host's thread as
//! an `Exchange` (`Exchange::metrics`). The run's decider notes each external cause it answers on
//! the recorder ([`Recorder::answered`], an [`ExternalAnswer`] with its [`AnswerSource`]); the
//! record of the request lists those its command asked. A reset clears no count and no record.
//! Without a recorder every path under the prefix answers `404`.

mod answer;
pub mod console;
#[cfg(feature = "listener")]
mod listener;
// The recording half (notes, records) is the listener's; without it only the reserved routes answer.
#[cfg_attr(not(feature = "listener"), allow(dead_code))]
mod observe;
mod router;
mod surface;
mod wire;

pub use answer::{Answer, answer, answered};
pub use console::Console;
#[cfg(feature = "listener")]
pub use listener::{BODY_READ, Exchange, HEADER_READ, Listener};
pub use observe::metrics::METRICS_MEDIA;
pub use observe::{
    AnswerSource, By, Class, ExternalAnswer, PANEL_MEDIA, PANEL_POLICY, REQUESTS_DEFAULT,
    REQUESTS_MOST, RESERVED_SEGMENT, Recorder, Reserved,
};
pub use router::{BODY_LIMIT, CONTENT_TYPE, Request, Response, Routed, route, route_with};
pub use surface::{Caller, Queried, Resolved, Route, Surface, SurfaceError, Unserved, Variable};

/// The `ingress.network` implementation this crate serves: HTTP on the loopback address.
pub const HTTP_LOOPBACK: &str = "http-loopback";

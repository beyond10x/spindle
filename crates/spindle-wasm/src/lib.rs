//! Spindle in a Service Worker: a specification's routes and the console routes answered from the
//! wasm host, so a static site's console never sends its fetch calls to a network.
//!
//! [`Spindle::open`] takes a [`Config`] as JSON: the specification's source files, compiled in the
//! browser by `spindle-spec`; an environment document, against which every component is admitted
//! as on any host; optionally a decisions file, which answers every `external:` outcome as `run
//! --decisions` answers it (`spindle_host::Answers`); optionally an `ess-http/1` binding,
//! compiled by ESS; and the name of the IndexedDB databases. A component whose state the
//! environment binds to `indexeddb` keeps it in a `BrowserStore` (`spindle-store-browser`)
//! of its own, `<database>/<component>`; one bound to `memory` keeps it in memory. Every command
//! and view is routed by `spindle-ingress` (`spindle_ingress::route`), the router the loopback
//! listener calls, so a request answers here as it answers there. Every write a request made has
//! landed in IndexedDB before its answer is returned.
//!
//! A component reached over the network must have `ingress.network` bound to [`SERVICE_WORKER`].
//! The console routes (`GET /api/model`, `GET /api/events`, `POST /api/reset`) are served beside
//! the specification's routes, as on a loopback listener: a Service Worker is reached only by the
//! pages of its own origin, which is the reach the loopback address gives the listener. A URL of
//! another origin is never answered: `Spindle::serves` says no and `Spindle::fetch` answers it
//! `404`. A reset that carries a world is refused; a reset without one empties every store.
//!
//! A status the Fetch standard gives a null body (`204`, `205`, `304`) is answered without one,
//! whatever the router wrote: the browser refuses to build such a `Response` with a body.
//!
//! A Service Worker is one instance per origin and scope, so this crate holds one `BrowserStore`
//! handle per database, as `spindle-store-browser` requires: two hosts open on the same
//! databases (two Service Workers of one origin with overlapping scopes, or a page opening the
//! same databases beside its worker) would each check versions against their own copy. Requests
//! are answered one at a time, in the order they arrived.
//!
//! The Service Worker shell and its registration are TypeScript, in `web/`.

/// The `ingress.network` implementation a Service Worker provides: requests from the pages of its
/// own origin, answered in the browser.
pub const SERVICE_WORKER: &str = "service-worker";

/// What [`Spindle::open`] opens a host with, as JSON.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The specification's source files, each as `[name, text]`: the name it is compiled under
    /// (`system.yaml`, `domains/<domain>.yaml`) and its text.
    pub sources: Vec<(String, String)>,
    /// The environment document's text (`spindle-environment/1`).
    pub environment: String,
    /// The decisions file's text, if external outcomes are answered from one.
    #[serde(default)]
    pub decisions: Option<String>,
    /// An `ess-http/1` binding's text, YAML or JSON, if the routes are bound by one.
    #[serde(default)]
    pub binding: Option<String>,
    /// The prefix of every IndexedDB database the host opens.
    pub database: String,
    /// The path the routes are served under, such as the Service Worker's scope; `/` without one.
    #[serde(default = "root")]
    pub base: String,
}

fn root() -> String {
    "/".to_owned()
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod worker;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub use worker::Spindle;

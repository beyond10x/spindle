//! Spindle in a Node program: a specification's routes and the console routes answered
//! in-process from the wasm host, every store in memory, so a Node server can mount it as an
//! end-to-end fake backend.
//!
//! `wasm-bindgen --target web` binds the module as an ES module whose glue imports nothing: Node
//! 22 loads it with `initSync({ module: bytes })` (or `await init({ module_or_path: bytes })`),
//! the bytes read by the program, so the module reads no file, opens no socket and starts no
//! thread.
//!
//! [`Spindle::create`] takes a [`Config`] as JSON or as a plain object: the specification's source
//! files, compiled in-process by `spindle-spec`; an environment document, against which every
//! component is admitted as on any host, each component's state bound to `memory` and its
//! `ingress.network`, where it has one, to [`NODE`]; optionally a decisions file and the answer
//! default, which answer every `external:` outcome as `run --decisions` and `run --answer-default`
//! answer it; and optionally an `ess-http/1` binding, compiled by ESS. The opening is
//! `spindle-embed`'s, the one the Service Worker host calls.
//!
//! [`Spindle::handle`] answers one request with `spindle_ingress::route_with`, the router the
//! loopback listener calls, the console routes (`GET /api/model`, `GET /api/events`,
//! `GET /api/served`, `POST /api/reset`) served beside the specification's routes; a reset may
//! carry a world, read as `run` reads it ([`spindle_host::read_world`]). [`Spindle::not_served`]
//! is the report `run` prints before it listens ([`spindle_ingress::Surface::report`]).
//!
//! Each call runs to its end before it returns, so requests are answered one at a time in the
//! order they arrive. Instances share nothing: two in one process keep two sets of stores.

/// The `ingress.network` implementation the Node export provides: requests handed to
/// [`Spindle::handle`] by the program that loaded the module.
pub const NODE: &str = "node";

/// What [`Spindle::create`] opens a host with, as JSON.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Config {
    /// The specification's source files, each as `[name, text]`: the name it is compiled under
    /// (`system.yaml`, `domains/<domain>.yaml`) and its text.
    pub sources: Vec<(String, String)>,
    /// The environment document's text (`spindle-environment/1`).
    pub environment: String,
    /// The decisions file's text, if external outcomes are answered from one.
    #[serde(default)]
    pub decisions: Option<String>,
    /// Whether each command's default branch answers an external outcome the decisions file does
    /// not, as `run --answer-default`.
    #[serde(default)]
    pub answer_default: bool,
    /// An `ess-http/1` binding's text, YAML or JSON, if the routes are bound by one.
    #[serde(default)]
    pub binding: Option<String>,
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod export;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub use export::Spindle;

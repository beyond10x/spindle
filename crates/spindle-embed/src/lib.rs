//! A host opened in-process from texts and answered one request at a time: what every wasm host
//! of Spindle shares (the Service Worker in `spindle-wasm`, the Node export in `spindle-node`).
//!
//! Nothing here reads a file, opens a socket, starts a thread or reads std's clock, so it builds
//! for `wasm32-unknown-unknown` as for a native target. A host opens in two steps:
//!
//! 1. [`admit`] compiles the specification's source files with `spindle-spec` and admits each
//!    component on the environment's text ([`spindle_host::admit`]). [`Admitted::placed`] then
//!    says what each component's state and `ingress.network` are bound to, so the caller can
//!    refuse what it does not run and open the stores it supplies (a Service Worker's IndexedDB
//!    stores open asynchronously, so this step is its own).
//! 2. [`Admitted::open`] reads the decisions file and the answer default as `run --decisions` and
//!    `run --answer-default` read them ([`spindle_host::Answers`]), compiles an `ess-http/1`
//!    binding with ESS, checks the fixed callers as `run --fixed-actor` checks them ([`fixed`]),
//!    lays out the routes with the console routes, each operation sent or read as the first fixed
//!    actor its grant names ([`spindle_ingress::Surface::fixing_callers`],
//!    [`spindle_ingress::Surface::with_console`]), and opens the host on the supplied stores
//!    ([`Host::supplied`]); a component whose state is bound to `memory` keeps it in memory.
//!
//! [`Opened::answer`] routes one request with `spindle_ingress::route_with`, the router the
//! loopback listener calls; where the opening allows it, a `POST /api/reset` carrying a world is
//! read by `run`'s world reader ([`spindle_host::read_world`]). [`answered`] maps the routed
//! response to the headers and body a transport writes.

use std::sync::Arc;

use ess_compiler::EssIr;
use ess_gen::http::Binding;
use serde_json::Value;
use spindle_capability::{Capability, Environment};
use spindle_host::{Answers, Clock, Host, Placements, Stores};
use spindle_ingress::{Caller, HTTP_LOOPBACK, Request, Response, Surface, Unfixed};

/// A specification compiled and admitted on an environment: every component placed, nothing
/// opened yet.
pub struct Admitted {
    ir: EssIr,
    environment: Environment,
    placements: Placements,
}

/// What one component's state and `ingress.network` are bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed<'a> {
    /// The component.
    pub component: &'a str,
    /// What `store.durable`, or without it `store.ephemeral`, is bound to; `None` where the
    /// component keeps no state.
    pub state: Option<Bound<'a>>,
    /// What `ingress.network` is bound to; `None` where the component is not reached over the
    /// network.
    pub network: Option<&'a str>,
}

/// One capability and the implementation admission bound for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bound<'a> {
    /// The capability, such as `store.durable`.
    pub capability: &'a str,
    /// The implementation, such as `memory`.
    pub implementation: &'a str,
}

/// Compiles the specification's source files, each `(name, text)`, and admits every component
/// on the environment `environment` (`spindle-environment/1`).
///
/// # Errors
/// The reason, as `the specification: ...` or `the environment: ...`, or the admission refusal
/// naming everything a component lacks.
pub fn admit(sources: Vec<(String, String)>, environment: &str) -> Result<Admitted, String> {
    let (names, texts): (Vec<String>, Vec<String>) = sources.into_iter().unzip();
    let ir = spindle_spec::compile(&names, &texts)
        .map_err(|error| format!("the specification: {error}"))?;
    let environment =
        Environment::parse(environment).map_err(|error| format!("the environment: {error}"))?;
    let placements =
        spindle_host::admit(&ir, &environment).map_err(|refused| refused.to_string())?;
    Ok(Admitted {
        ir,
        environment,
        placements,
    })
}

/// How [`Admitted::open`] opens the host.
pub struct Opening {
    /// A store for each component whose state is bound to something other than `memory`, by
    /// component.
    pub stores: Stores,
    /// The components whose routes are served.
    pub reached: Vec<String>,
    /// The decisions file's text, if external outcomes are answered from one.
    pub decisions: Option<String>,
    /// Each command's default branch answers what the decisions file does not, as
    /// `run --answer-default`.
    pub answer_default: bool,
    /// An `ess-http/1` binding's text, YAML or JSON, if the routes are bound by one.
    pub binding: Option<String>,
    /// The fixed actors, each with its caller attributes, in order, as `run --fixed-actor` with
    /// `--fixed-caller` takes them: each served operation is sent or read as the first its grant
    /// names ([`Surface::fixing_callers`]), a test-backend switch, not authentication; with none,
    /// as no caller.
    pub callers: Vec<Caller>,
    /// Whether a `POST /api/reset` may carry a world; without, one that does is refused `400`.
    pub worlds: bool,
    /// The clock the host reads.
    pub clock: Arc<dyn Clock>,
}

impl Admitted {
    /// The compiled specification.
    pub fn ir(&self) -> &EssIr {
        &self.ir
    }

    /// What each component's state and `ingress.network` are bound to, by component name.
    pub fn placed(&self) -> Vec<Placed<'_>> {
        self.placements
            .iter()
            .map(|(component, bindings)| {
                let bound = |capability: &str| {
                    bindings
                        .iter()
                        .find(|binding| binding.capability.as_str() == capability)
                        .map(|binding| Bound {
                            capability: binding.capability.as_str(),
                            implementation: binding.implementation.as_str(),
                        })
                };
                Placed {
                    component,
                    state: bound(Capability::STORE_DURABLE).or_else(|| bound("store.ephemeral")),
                    network: bound(Capability::INGRESS_NETWORK).map(|bound| bound.implementation),
                }
            })
            .collect()
    }

    /// Opens the host `opening` describes.
    ///
    /// With a decisions file or `answer_default`, every external cause is answered by them
    /// (`Answers::served`); with neither, the host answers none, and a specification that
    /// declares one is refused.
    ///
    /// Each fixed caller is checked as `run` checks it before listening ([`fixed`]).
    ///
    /// # Errors
    /// The reason, as `the decisions file: ...`, `the binding: ...`,
    /// `the binding was refused: ...`, `the binding cannot be served: ...`, a fixed caller's
    /// refusal ([`fixed`]), `the routes: ...`, or the host's refusal.
    pub fn open(self, opening: Opening) -> Result<Opened, String> {
        let Admitted {
            ir, environment, ..
        } = self;
        let file = opening
            .decisions
            .as_deref()
            .map(|text| Answers::parse(&ir, text))
            .transpose()
            .map_err(|error| format!("the decisions file: {error}"))?;
        let decides = file.is_some() || opening.answer_default;
        let file = file.unwrap_or_default();
        let answers = if opening.answer_default {
            file.taking_defaults(&ir)
        } else {
            file
        };
        let binding = opening
            .binding
            .map(|text| binding(&ir, &text))
            .transpose()?;
        fixed(&ir, &opening.callers)?;
        let surface =
            Surface::fixing_callers(&ir, &opening.reached, binding.as_ref(), &opening.callers)
                .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
                .map_err(|refused| format!("the routes: {refused}"))?;
        let decider = decides.then(|| answers.clone().served());
        let host = Host::supplied(ir, &environment, opening.clock, decider, opening.stores)
            .map_err(|error| error.to_string())?;
        Ok(Opened {
            surface,
            host,
            answers,
            worlds: opening.worlds,
        })
    }
}

/// A host open on its stores with the routes it serves.
pub struct Opened {
    surface: Surface,
    host: Host,
    /// The run's answers, beneath the decision table a reset's world carries.
    answers: Answers,
    worlds: bool,
}

impl Opened {
    /// The routes served.
    pub fn surface(&self) -> &Surface {
        &self.surface
    }

    /// Answers `request` as the loopback listener answers it. A reset's world is read as `run`
    /// reads it, its decision table laid over the run's answers until the next reset, where the
    /// opening allowed worlds.
    pub fn answer(&mut self, request: Request) -> Response {
        if !self.worlds {
            return spindle_ingress::route(&self.surface, &mut self.host, request).response;
        }
        let answers = &self.answers;
        let worlds = |ir: &EssIr, body: &Value| {
            spindle_host::read_world(ir, body, |table| table.over(answers.clone()))
        };
        spindle_ingress::route_with(&self.surface, &mut self.host, request, &worlds).response
    }
}

/// An `ess-http/1` binding's text, YAML or JSON, compiled by ESS against `ir`.
///
/// # Errors
/// The reason, as `the binding: ...`, `the binding was refused: ...` or
/// `the binding cannot be served: ...`.
pub fn binding(ir: &EssIr, text: &str) -> Result<Binding, String> {
    let spec = if serde_json::from_str::<Value>(text).is_ok() {
        ess_http::HttpSpec::from_json(text).map_err(|error| error.to_string())
    } else {
        ess_http::HttpSpec::from_yaml(text).map_err(|error| error.to_string())
    }
    .map_err(|error| format!("the binding: {error}"))?;
    let compiled = ess_http::compile(&spec, ir, &ess_gen::http::derived(ir))
        .map_err(|diagnostics| format!("the binding was refused:\n{diagnostics}"))?;
    Binding::new(compiled).map_err(|open| format!("the binding cannot be served:\n{open}"))
}

/// Refuses the fixed callers `callers`, in order, as `run` refuses its `--fixed-actor` and
/// `--fixed-caller` before listening ([`Caller::repeated`], [`Caller::fixed`]): an actor given
/// twice, an actor `ir` does not declare, or attributes that are not a caller of that actor's.
///
/// # Errors
/// `run`'s reason after the actor named as this crate's hosts name it:
/// `the fixed actor <actor> is given twice; ...`,
/// `the fixed actor <actor>: the specification declares no such actor`, or
/// `the caller of the fixed actor <actor>: <the kernel's refusal>`.
pub fn fixed(ir: &EssIr, callers: &[Caller]) -> Result<(), String> {
    let named = |refused: Unfixed| match refused {
        Unfixed::Twice { .. } => format!("the fixed actor {} {refused}", refused.actor()),
        Unfixed::Undeclared { .. } => format!("the fixed actor {}: {refused}", refused.actor()),
        Unfixed::Unbound { .. } => {
            format!(
                "the caller of the fixed actor {}: {refused}",
                refused.actor()
            )
        }
    };
    Caller::repeated(callers).map_err(named)?;
    callers
        .iter()
        .try_for_each(|caller| caller.fixed(ir))
        .map_err(named)
}

/// The statuses the Fetch standard gives a null body: a browser refuses to build a `Response`
/// with one and a body, so none is written for them.
pub const NULL_BODY: [u16; 5] = [101, 103, 204, 205, 304];

/// A routed response as a transport writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answered<'a> {
    /// The headers, each name once and lower-case where this crate writes it: the content type
    /// where there is a body (the panel's or the metrics' media type where the response says it
    /// is one of them, [`spindle_ingress::CONTENT_TYPE`] otherwise), the panel's
    /// `content-security-policy`, `allow` where the response carries it, then each header the
    /// binding declares whose name none before it has.
    pub headers: Vec<(String, String)>,
    /// The body, `None` where it is empty or the status takes none ([`NULL_BODY`]).
    pub body: Option<&'a str>,
}

/// `response` as a transport writes it ([`Answered`]).
pub fn answered(response: &Response) -> Answered<'_> {
    let body = (!response.body.is_empty() && !NULL_BODY.contains(&response.status))
        .then_some(response.body.as_str());
    let mut headers: Vec<(String, String)> = Vec::new();
    if response.panel {
        headers.push((
            "content-type".to_owned(),
            spindle_ingress::PANEL_MEDIA.to_owned(),
        ));
        headers.push((
            "content-security-policy".to_owned(),
            spindle_ingress::PANEL_POLICY.to_owned(),
        ));
    } else if response.metrics && body.is_some() {
        headers.push((
            "content-type".to_owned(),
            spindle_ingress::METRICS_MEDIA.to_owned(),
        ));
    } else if body.is_some() {
        headers.push((
            "content-type".to_owned(),
            spindle_ingress::CONTENT_TYPE.to_owned(),
        ));
    }
    if let Some(allow) = &response.allow {
        headers.push(("allow".to_owned(), allow.clone()));
    }
    for (name, value) in &response.headers {
        if !headers
            .iter()
            .any(|(written, _)| written.eq_ignore_ascii_case(name))
        {
            headers.push((name.clone(), value.clone()));
        }
    }
    Answered { headers, body }
}

#[cfg(test)]
mod tests {
    use super::{NULL_BODY, answered};
    use spindle_ingress::Response;

    fn response(status: u16, body: &str) -> Response {
        Response {
            status,
            allow: None,
            panel: false,
            metrics: false,
            headers: Vec::new(),
            body: body.to_owned(),
        }
    }

    #[test]
    fn a_null_body_status_is_answered_without_a_body_or_a_content_type() {
        for status in NULL_BODY {
            let answer = response(status, "{\"x\":1}");
            let mapped = answered(&answer);
            assert_eq!(mapped.body, None, "{status}");
            assert!(mapped.headers.is_empty(), "{status}: {:?}", mapped.headers);
        }
    }

    #[test]
    fn a_declared_header_never_replaces_one_written_before_it() {
        let mut answer = response(405, "{}");
        answer.allow = Some("POST".to_owned());
        answer.headers = vec![
            ("Allow".to_owned(), "GET".to_owned()),
            ("location".to_owned(), "/x".to_owned()),
            ("Location".to_owned(), "/y".to_owned()),
        ];
        let mapped = answered(&answer);
        assert_eq!(
            mapped.headers,
            vec![
                (
                    "content-type".to_owned(),
                    spindle_ingress::CONTENT_TYPE.to_owned()
                ),
                ("allow".to_owned(), "POST".to_owned()),
                ("location".to_owned(), "/x".to_owned()),
            ]
        );
        assert_eq!(mapped.body, Some("{}"));
    }
}

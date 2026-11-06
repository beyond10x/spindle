//! The routes one listener serves, read from ESS: the routes `ess generate --kind openapi`
//! publishes, with an `ess-http/1` binding's in place of those it names.

use std::collections::BTreeMap;
use std::fmt;

use ess_compiler::EssIr;
use ess_gen::http::{Binding, Flat};
use ess_http::{Body, Encoding, Method, Operation, ResponseEncoding, Segment, ViewBody};
use serde_json::{Map, Value};

use crate::console::Console;

/// One path variable of a route: the input field it carries and how its text is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    /// Its position among the path's segments.
    pub position: usize,
    /// The input field or view parameter it carries.
    pub field: String,
    /// How its text is read ([`ess_gen::http::scalar`]).
    pub flat: Flat,
}

/// One route: a method and path of a served component and the command or view behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// The component whose surface it is on.
    pub component: String,
    /// The method.
    pub method: Method,
    /// The path template: the binding's, or the one ESS derives for an operation no binding
    /// names.
    pub path: String,
    /// The command or view it serves.
    pub operation: Operation,
    segments: Vec<Segment>,
    variables: Vec<Variable>,
    statuses: BTreeMap<String, u16>,
    input: Vec<String>,
    /// The outcomes answered with the command's response beside the envelope.
    responding: Vec<String>,
    unserved: Option<Unserved>,
}

/// Why a route is kept but its operation not served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unserved {
    /// The command's route uses what this ingress does not serve, named. A request for it is
    /// answered `501` before anything is read or written. A view this ingress cannot route is
    /// not kept: its contract declares no `501`, so [`Surface::new`] refuses it.
    Ingress(String),
    /// The kernel's [`runs`](spindle_kernel::runs) refuses the command, as its text. A request
    /// for it is answered as before this was known: the host refuses it before anything is read
    /// or written, `501` naming the construct.
    Kernel(String),
    /// The host's [`reads`](spindle_host::reads) refuses the view, as its text. A request for it
    /// is answered as before this was known: the host refuses it before reading anything, `501`
    /// naming the construct.
    Host(String),
}

impl fmt::Display for Unserved {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ingress(what) => write!(formatter, "this ingress does not serve {what}"),
            Self::Kernel(refused) | Self::Host(refused) => formatter.write_str(refused),
        }
    }
}

impl Route {
    /// Why the operation is not served; `None` for one that is.
    pub fn unserved(&self) -> Option<&Unserved> {
        self.unserved.as_ref()
    }

    /// The status the command's outcome `outcome` answers: the binding's where it lists one, ESS's
    /// derived status otherwise.
    pub fn status(&self, outcome: &str) -> Option<u16> {
        self.statuses.get(outcome).copied()
    }

    /// Whether the command's outcome `outcome` answers with the command's response under
    /// `response`, beside the envelope: a branch that returns it, from `ess/22`
    /// ([`ess_http::status::answers_with_response`]).
    pub(crate) fn responds(&self, outcome: &str) -> bool {
        self.responding
            .iter()
            .any(|responding| responding == outcome)
    }

    /// The path's variables.
    pub fn variables(&self) -> &[Variable] {
        &self.variables
    }

    /// The input fields a command declares, by name: its published `Input` is closed, so a
    /// request carrying any other is refused. Empty for a view.
    pub fn input(&self) -> &[String] {
        &self.input
    }

    /// The input `path` carries in its variables, read as [`ess_gen::http::Flat`] says; `None`
    /// when `path` does not match the template or leaves a variable's segment empty (ESS's
    /// served application matches only a variable holding a segment that is not empty),
    /// `Some(Err)` when a variable's text is malformed.
    fn carried(&self, path: &str) -> Option<Result<Map<String, Value>, String>> {
        if !ess_gen::http::template_matches(&self.segments, path) {
            return None;
        }
        let given: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        if self.variables.iter().any(|variable| {
            given
                .get(variable.position)
                .is_none_or(|text| text.is_empty())
        }) {
            return None;
        }
        if matches!(self.unserved, Some(Unserved::Ingress(_))) {
            return Some(Ok(Map::new()));
        }
        let mut carried = Map::new();
        for variable in &self.variables {
            let Some(text) = given.get(variable.position).and_then(|text| decoded(text)) else {
                return Some(Err(format!(
                    "the path segment for {} is not percent-encoded UTF-8",
                    variable.field
                )));
            };
            match read(variable.flat, text) {
                Some(value) => carried.insert(variable.field.clone(), value),
                None => return Some(Err(format!("the path carries no {}", variable.field))),
            };
        }
        Some(Ok(carried))
    }
}

/// A path segment with every `%XX` decoded, as ESS's served application decodes it; `None` where
/// an escape is not two hexadecimal digits or the bytes are not UTF-8. A `+` stays a `+`.
fn decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = char::from(*bytes.get(index + 1)?).to_digit(16)?;
            let low = char::from(*bytes.get(index + 2)?).to_digit(16)?;
            out.push(u8::try_from(high * 16 + low).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// One scalar read from flat text, as ESS's served applications read it: any text for a
/// `String`; for anything else empty text is malformed (`None`), an `Integer` in exact decimal
/// text a JSON number and a `Boolean` `true` or `false`, every other text left a string for the
/// kernel's input check to refuse by name.
fn read(flat: Flat, text: String) -> Option<Value> {
    match flat {
        Flat::String => Some(Value::String(text)),
        _ if text.is_empty() => None,
        Flat::Integer if whole(&text) => Some(
            serde_json::from_str::<serde_json::Number>(&text)
                .map_or(Value::String(text), Value::Number),
        ),
        Flat::Integer => Some(Value::String(text)),
        Flat::Boolean => Some(match text.as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => Value::String(text),
        }),
        Flat::Text => Some(Value::String(text)),
    }
}

/// Exact decimal text: an optional `-` and one or more ASCII digits.
fn whole(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// What a request's method and path resolve to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved<'a> {
    /// The route the request is for, and the input its path carries.
    Found(&'a Route, Map<String, Value>),
    /// The path matches a route whose variable it holds malformed.
    Malformed(String),
    /// A console route ([`Surface::with_console`]).
    Console(Console),
    /// The path is served, under these methods only.
    WrongMethod(Vec<Method>),
    /// No route has the path.
    NotFound,
}

/// Why a surface cannot be served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceError {
    /// The specification declares no component of this name.
    NoComponent(String),
    /// Two served components claim one method and path.
    Collision {
        /// The method.
        method: Method,
        /// The path.
        path: String,
        /// The components that both claim it.
        components: [String; 2],
    },
    /// A route the binding or ESS publishes would be shadowed by a console route.
    Shadowed {
        /// The console route.
        console: Console,
        /// The command or view whose route it would shadow.
        operation: String,
        /// That route's method.
        method: Method,
        /// That route's path template.
        path: String,
    },
    /// The route of an operation uses what this ingress does not serve, named.
    Unsupported {
        /// The command or view.
        operation: String,
        /// What it uses.
        what: String,
    },
}

impl fmt::Display for SurfaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoComponent(name) => write!(formatter, "no component {name}"),
            Self::Collision {
                method,
                path,
                components: [first, second],
            } => write!(
                formatter,
                "{first} and {second} both claim {} {path}; one listener cannot serve both",
                method.as_str().to_uppercase()
            ),
            Self::Shadowed {
                console,
                operation,
                method,
                path,
            } => write!(
                formatter,
                "the console route {console} would shadow {operation}, which is served at {} {path}; the console's paths are fixed, so the binding must serve it at another path",
                method.as_str().to_uppercase()
            ),
            Self::Unsupported { operation, what } => {
                write!(formatter, "{operation}: this ingress does not serve {what}")
            }
        }
    }
}

impl std::error::Error for SurfaceError {}

/// Every route of the served components, component by component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Surface {
    routes: Vec<Route>,
    console: bool,
}

impl Surface {
    /// The surface of `components`: for each, the operations ESS derives for it, its accepted
    /// commands and, where it is `reached_by: network`, the views of the domains it owns.
    ///
    /// Each operation is served where [`ess_gen::http::bound_routes`] puts it, which is where
    /// `ess generate --kind openapi` (with `--http`, where a binding is given) publishes it:
    /// without a binding, or for an operation the binding does not name, ESS's derived route and
    /// status; for one it names, the binding's method and path, its path variables read into the
    /// input, and the status it lists for an outcome ([`ess_gen::http::listed`]).
    ///
    /// A command this ingress cannot route keeps its route and is marked [`Unserved::Ingress`]:
    /// a `form` or `query` request, an outcome answered other than with the envelope (a body,
    /// headers, a redirect or text), a standard refusal the binding answers its own way, an
    /// outcome that answers with the command's retained result, or a path variable that is not a
    /// scalar. A command the kernel's [`runs`](spindle_kernel::runs) refuses is marked
    /// [`Unserved::Kernel`], and a view the host's [`reads`](spindle_host::reads) refuses
    /// [`Unserved::Host`].
    ///
    /// # Errors
    /// [`SurfaceError::NoComponent`] for a name `ir` does not declare,
    /// [`SurfaceError::Collision`] when two components claim one route, and
    /// [`SurfaceError::Unsupported`] for a binding that answers a standard refusal its own way
    /// for every operation, a path that is not a template, or a view this ingress cannot route
    /// (one answered other than as `rows`, a standard refusal the binding answers its own way, a
    /// path variable that is not a scalar): ESS publishes no `501` for a view, so it is refused
    /// rather than answered with a status its contract does not declare.
    pub fn new(
        ir: &EssIr,
        components: &[String],
        binding: Option<&Binding>,
    ) -> Result<Self, SurfaceError> {
        if let Some(binding) = binding
            && let Some(key) = binding.ir().refusals().keys().next()
        {
            return Err(SurfaceError::Unsupported {
                operation: "the binding".to_owned(),
                what: format!("its own answer to the standard refusal {key}"),
            });
        }
        let mut routes: Vec<Route> = Vec::new();
        for name in components {
            let component = ir
                .components()
                .values()
                .find(|component| component.name.as_str() == name)
                .ok_or_else(|| SurfaceError::NoComponent(name.clone()))?;
            for bound in ess_gen::http::bound_routes(ir, component, binding) {
                let route = route(ir, name, &bound)?;
                if let Some(taken) = routes
                    .iter()
                    .find(|taken| taken.method == route.method && taken.path == route.path)
                {
                    return Err(SurfaceError::Collision {
                        method: route.method,
                        path: route.path,
                        components: [taken.component.clone(), route.component],
                    });
                }
                routes.push(route);
            }
        }
        Ok(Self {
            routes,
            console: false,
        })
    }

    /// This surface with the [`Console`] routes added where `implementation`, what
    /// `ingress.network` is bound to, is [`HTTP_LOOPBACK`](crate::HTTP_LOOPBACK); for any other
    /// implementation the surface is returned as it is, and no console route exists.
    ///
    /// # Errors
    /// [`SurfaceError::Shadowed`] when a route the binding or ESS publishes has a path a console
    /// route also has, under any method: the console never shadows one.
    pub fn with_console(mut self, implementation: &str) -> Result<Self, SurfaceError> {
        if implementation != crate::HTTP_LOOPBACK {
            return Ok(self);
        }
        for console in Console::ALL {
            if let Some(route) = self
                .routes
                .iter()
                .find(|route| route.carried(console.path()).is_some())
            {
                return Err(SurfaceError::Shadowed {
                    console,
                    operation: route.operation.to_string(),
                    method: route.method,
                    path: route.path.clone(),
                });
            }
        }
        self.console = true;
        Ok(self)
    }

    /// Every route, component by component.
    pub fn routes(&self) -> &[Route] {
        &self.routes
    }

    /// Every route whose operation is not served, component by component and, within one, in
    /// name order: commands, then views, each by qualified name, as the compiled model's maps
    /// hold them.
    pub fn unserved(&self) -> Vec<&Route> {
        let mut unserved: Vec<&Route> = self
            .routes
            .iter()
            .filter(|route| route.unserved.is_some())
            .collect();
        let component = |route: &Route| {
            self.routes
                .iter()
                .position(|first| first.component == route.component)
        };
        unserved.sort_by_cached_key(|route| (component(route), route.operation.to_string()));
        unserved
    }

    /// The route a request with `method` (compared exactly, RFC 9110 §9.1) and `path` is for.
    /// Compiling the binding
    /// refused two templates a request path can both match (`ESS-HTTP-010`, `ESS-HTTP-011`), so
    /// at most one route of a method matches. A console route, where the surface has them, is
    /// matched exactly; no other route shares its path ([`Surface::with_console`]).
    pub fn resolve(&self, method: &str, path: &str) -> Resolved<'_> {
        if self.console
            && let Some(console) = Console::ALL
                .into_iter()
                .find(|console| console.path() == path)
        {
            return if console.method().as_str().to_uppercase() == method {
                Resolved::Console(console)
            } else {
                Resolved::WrongMethod(vec![console.method()])
            };
        }
        let mut served = Vec::new();
        for route in &self.routes {
            let Some(carried) = route.carried(path) else {
                continue;
            };
            if route.method.as_str().to_uppercase() != method {
                served.push(route.method);
                continue;
            }
            return match carried {
                Ok(carried) => Resolved::Found(route, carried),
                Err(why) => Resolved::Malformed(why),
            };
        }
        if served.is_empty() {
            Resolved::NotFound
        } else {
            Resolved::WrongMethod(served)
        }
    }
}

/// One route of `component`, owned: served, or kept and marked [`Unserved`] with what it uses
/// that this ingress, the kernel or the host does not run.
///
/// # Errors
/// [`SurfaceError::Unsupported`] when the route's path is not a template ESS reads, so that no
/// request can be matched to it, or for a view this ingress cannot route.
fn route(
    ir: &EssIr,
    component: &str,
    bound: &ess_gen::http::Route<'_>,
) -> Result<Route, SurfaceError> {
    let operation = bound.serves.operation();
    let path = bound.path.clone();
    let segments = ess_http::template(&path).map_err(|reasons| SurfaceError::Unsupported {
        operation: operation.to_string(),
        what: format!("the path {path}: {reasons:?}"),
    })?;
    let mut route = Route {
        component: component.to_owned(),
        method: bound.method.binding(),
        path,
        operation,
        segments,
        variables: Vec::new(),
        statuses: BTreeMap::new(),
        input: Vec::new(),
        responding: Vec::new(),
        unserved: None,
    };
    match (served(ir, bound, &mut route), &route.operation) {
        (Err(what), Operation::View(_)) => {
            return Err(SurfaceError::Unsupported {
                operation: route.operation.to_string(),
                what,
            });
        }
        (Err(what), Operation::Command(_)) => {
            route.variables.clear();
            route.statuses.clear();
            route.responding.clear();
            route.unserved = Some(Unserved::Ingress(what));
        }
        (Ok(()), Operation::Command(_)) => {
            if let ess_gen::http::Served::Command(handle) = bound.serves
                && let Err(refused) = spindle_kernel::runs(ir, ir.command(handle))
            {
                route.unserved = Some(Unserved::Kernel(refused.to_string()));
            }
        }
        (Ok(()), Operation::View(view)) => {
            if let Err(refused) = spindle_host::reads(ir, view) {
                route.unserved = Some(Unserved::Host(refused.to_string()));
            }
        }
    }
    Ok(route)
}

/// Fills `route`'s variables, input and statuses; `Err` names what it uses that this ingress
/// does not serve: a `form` or `query` request, an outcome answered other than with the envelope
/// (a body, headers, a redirect or text), a view answered other than as `rows`, a standard
/// refusal the binding answers its own way, a command whose outcome answers with its retained result,
/// a path variable that is not a scalar, or a command that reads its caller: how an HTTP caller
/// proves its attributes is an authentication technical not yet decided (design §11).
fn served(ir: &EssIr, bound: &ess_gen::http::Route<'_>, route: &mut Route) -> Result<(), String> {
    for (variable, field) in ess_gen::http::path_fields(ir, bound) {
        let position = route
            .segments
            .iter()
            .position(|segment| *segment == Segment::Variable(variable.clone()))
            .ok_or_else(|| format!("the variable {variable}"))?;
        let flat = ess_gen::http::scalar(ir, &field.type_ref)
            .ok_or_else(|| format!("{} in the path, which is not a scalar", field.name))?;
        route.variables.push(Variable {
            position,
            field: field.name.clone(),
            flat,
        });
    }
    if let Some(declared) = bound.declared
        && let Some(key) = declared.refusals.keys().next()
    {
        return Err(format!("its own answer to the standard refusal {key}"));
    }
    match bound.serves {
        ess_gen::http::Served::Command(handle) => {
            let read = spindle_kernel::caller_reads(ir, ir.command(handle));
            if !read.is_empty() {
                return Err(format!(
                    "the caller attributes it reads ({}), which no HTTP caller proves here",
                    read.iter()
                        .map(|attribute| format!("`{attribute}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            route.input = ir
                .command(handle)
                .input
                .iter()
                .map(|field| field.name.clone())
                .collect();
            if ess_gen::http::request(bound).encoding != Encoding::Json {
                return Err("a request that is not a JSON body".to_owned());
            }
            for outcome in &ir.command(handle).outcomes {
                if outcome.retains_result {
                    return Err(format!(
                        "{}, which answers with the command's response",
                        outcome.name
                    ));
                }
                if ess_http::status::answers_with_response(ir, outcome) {
                    route.responding.push(outcome.name.to_string());
                }
                let status = match ess_gen::http::listed(bound, outcome) {
                    Some(listed) => {
                        if listed.body != Body::Envelope
                            || listed.encoding != ResponseEncoding::default()
                            || !listed.headers.is_empty()
                            || listed.redirect.is_some()
                        {
                            return Err(format!(
                                "{} answered other than with its status and the envelope",
                                outcome.name
                            ));
                        }
                        listed.status
                    }
                    None => ess_http::status::outcome_status(ir, outcome)
                        .parse()
                        .map_err(|_| "an outcome without a status".to_owned())?,
                };
                route.statuses.insert(outcome.name.to_string(), status);
            }
        }
        ess_gen::http::Served::View(_) => {
            if bound
                .declared
                .is_some_and(|declared| declared.body != ViewBody::Rows)
            {
                return Err("a view answered other than as rows".to_owned());
            }
        }
    }
    Ok(())
}

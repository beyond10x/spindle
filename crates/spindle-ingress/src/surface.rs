//! The routes one listener serves, read from ESS: the routes `ess generate --kind openapi`
//! publishes, with an `ess-http/1` binding's in place of those it names.

use std::collections::BTreeMap;
use std::fmt;

use ess_compiler::EssIr;
use ess_gen::http::{Binding, Flat, Shape};
use ess_http::{Body, Encoding, Method, Operation, ResponseEncoding, Segment, Unknown, ViewBody};
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

/// One parameter of a view read from the query string: the parameter, its wire name and how its
/// text is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Queried {
    /// The view parameter it carries.
    pub field: String,
    /// The query key it is read under: the parameter's wire name.
    pub wire: String,
    /// How its text is read ([`ess_gen::http::scalar`]).
    pub flat: Flat,
}

/// One input field of a `form` or `query` request: the key it is sent under, the field it fills,
/// how its text is read ([`ess_gen::http::flat`]), and whether the path carries it instead.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Parameter {
    wire: String,
    field: String,
    flat: Flat,
    shape: Shape,
    carried: bool,
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
    queried: Vec<Queried>,
    statuses: BTreeMap<String, u16>,
    input: Vec<String>,
    encoding: Encoding,
    unknown: Unknown,
    parameters: Vec<Parameter>,
    /// The outcomes answered with the command's response beside the envelope.
    responding: Vec<String>,
    unserved: Option<Unserved>,
    caller: Option<Caller>,
    not_granted: bool,
}

/// The actor and caller every command of a surface is sent as, fixed for the whole run
/// ([`Surface::fixing_caller`]): a test-backend switch, not authentication. A request proves
/// nothing about who sends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    /// The qualified name of the actor.
    pub actor: String,
    /// The caller's attributes, by declared attribute name.
    pub attributes: Map<String, Value>,
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

    /// The view parameters read from the query string: every one the path does not carry. Empty
    /// for a command.
    pub fn queried(&self) -> &[Queried] {
        &self.queried
    }

    /// The view parameters `query`, a request's query string, carries, by parameter name, as
    /// ESS's served application reads them (`http::query_object`): each key and value
    /// form-decoded (`+` and `%XX`), a key no parameter has as its wire name ignored, a value read
    /// as [`ess_gen::http::Flat`] says. A parameter the query leaves out is left out, for the host
    /// to refuse where it is required.
    ///
    /// # Errors
    /// Naming the parameter, under `query.`: a key given more than once, a value that is not
    /// percent-encoded UTF-8, or an empty value for a parameter that is not a `String`.
    pub fn parameters(&self, query: &str) -> Result<Map<String, Value>, String> {
        let mut read = Map::new();
        for queried in &self.queried {
            let found: Vec<&str> = query
                .split('&')
                .filter(|pair| !pair.is_empty())
                .map(|pair| pair.split_once('=').unwrap_or((pair, "")))
                .filter(|(key, _)| form_decoded(key).as_deref() == Some(queried.wire.as_str()))
                .map(|(_, value)| value)
                .collect();
            let place = format!("query.{}", queried.wire);
            let raw = match found.as_slice() {
                [] => continue,
                [raw] => *raw,
                several => {
                    return Err(format!(
                        "{place}: expected one value, found {}",
                        several.len()
                    ));
                }
            };
            let text = form_decoded(raw).ok_or_else(|| {
                format!("{place}: expected percent-encoded UTF-8 text, found `{raw}`")
            })?;
            let value = self::read(queried.flat, text)
                .ok_or_else(|| format!("{place}: expected a value, found empty text"))?;
            read.insert(queried.field.clone(), value);
        }
        Ok(read)
    }

    /// The input fields a command declares, by name: its published `Input` is closed, so a
    /// request carrying any other is refused. Empty for a view.
    pub fn input(&self) -> &[String] {
        &self.input
    }

    /// How a command's request carries its input ([`ess_gen::http::request`]): ESS's own `json`
    /// body, an `application/x-www-form-urlencoded` body, or the query string.
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// A `form` body or a query string, `text`, read as the command's input, as ESS's served
    /// application reads it (`ess-synth` `http::flat_object`): keys and values form-decoded (`+`
    /// and `%XX`, UTF-8), each value read by its field's [`Flat`], a `List` field the array of
    /// its key's values (empty when absent), any other field present once at most. A key no field
    /// declares, the path carries, or that does not decode is refused naming it under `at`, or
    /// dropped where the binding says `unknown: ignore`.
    pub(crate) fn flat_input(&self, text: &str, at: &str) -> Result<Map<String, Value>, String> {
        let ignore = self.unknown == Unknown::Ignore;
        let mut found: Vec<Vec<String>> = vec![Vec::new(); self.parameters.len()];
        for pair in text.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let Some(key) = form_decoded(key) else {
                if ignore {
                    continue;
                }
                return Err(format!("{at}: a key is not percent-encoded UTF-8: `{key}`"));
            };
            let place = format!("{at}.{key}");
            let index = self
                .parameters
                .iter()
                .position(|parameter| parameter.wire == key);
            let carried = index.is_some_and(|index| self.parameters[index].carried);
            let Some(index) = index.filter(|_| !carried) else {
                if ignore {
                    continue;
                }
                return Err(if carried {
                    format!("{place}: the path carries this field, so it is not a parameter here")
                } else {
                    format!("{place}: no field of this request is named so")
                });
            };
            let value = form_decoded(value).ok_or_else(|| {
                format!("{place}: expected percent-encoded UTF-8 text, found `{value}`")
            })?;
            found[index].push(value);
        }
        let mut input = Map::new();
        for (parameter, values) in self.parameters.iter().zip(found) {
            if parameter.carried {
                continue;
            }
            let place = format!("{at}.{}", parameter.wire);
            let flat = |text: String| {
                read(parameter.flat, text)
                    .ok_or_else(|| format!("{place}: expected a value, found the empty text"))
            };
            match parameter.shape {
                Shape::List => {
                    let items = values
                        .into_iter()
                        .map(flat)
                        .collect::<Result<Vec<_>, String>>()?;
                    input.insert(parameter.field.clone(), Value::Array(items));
                }
                Shape::One | Shape::Optional => match values.len() {
                    0 => {}
                    1 => {
                        let value = values.into_iter().next().unwrap_or_default();
                        input.insert(parameter.field.clone(), flat(value)?);
                    }
                    several => {
                        return Err(format!("{place}: expected one value, found {several}"));
                    }
                },
            }
        }
        Ok(input)
    }

    /// The caller a command is sent as, where the surface fixes one; `None` for none.
    pub fn caller(&self) -> Option<&Caller> {
        self.caller.as_ref()
    }

    /// Whether the route's view is read-granted (ess/22 `may:` naming a view, `x-ess-may-read`)
    /// and the fixed caller's actor may not read it: ESS's served surface then refuses it `403`
    /// "not granted" before reading anything. Always `false` without a fixed caller.
    pub fn not_granted(&self) -> bool {
        self.not_granted
    }

    /// The input `path` carries in its variables, read as [`ess_gen::http::Flat`] says; `None`
    /// when `path` does not match the template or leaves a variable's segment empty (ESS's
    /// served application matches only a variable holding a segment that is not empty),
    /// `Some(Err)` when a variable's text is malformed. Nothing is read from a matched path for a
    /// command the ingress does not serve or a view the fixed caller may not read: both are
    /// refused before their request is read.
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
        if matches!(self.unserved, Some(Unserved::Ingress(_))) || self.not_granted {
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

/// A `form` or query key or value with every `+` a space and every `%XX` decoded, as ESS's
/// served application decodes one; `None` where an escape is not two hexadecimal digits or the
/// bytes are not UTF-8.
fn form_decoded(text: &str) -> Option<String> {
    decoded(&text.replace('+', " "))
}

/// One scalar read from flat text, as ESS's served applications read it: any text for a
/// `String`; for anything else empty text is malformed (`None`), an `Integer` in exact decimal
/// text a JSON number and a `Boolean` `true` or `false`, every other text left a string for the
/// kernel's input check to refuse by name.
fn read(flat: Flat, text: String) -> Option<Value> {
    match flat {
        Flat::String => Some(Value::String(text)),
        _ if text.is_empty() => None,
        Flat::Integer if whole(&text) => {
            Some(text.parse::<i64>().map_or(Value::String(text), Value::from))
        }
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
    /// input, the request it declares (`json`, `form` or `query`) and the status it lists for an
    /// outcome ([`ess_gen::http::listed`]).
    ///
    /// A command this ingress cannot route keeps its route and is marked [`Unserved::Ingress`]:
    /// an outcome answered other than with the envelope (a body, headers, a redirect or text), a
    /// standard refusal the binding answers its own way, an outcome that answers with the
    /// command's retained result, or a path variable that is not a scalar. A command the kernel's [`runs`](spindle_kernel::runs) refuses is marked
    /// [`Unserved::Kernel`], and a view the host's [`reads`](spindle_host::reads) refuses
    /// [`Unserved::Host`].
    ///
    /// # Errors
    /// [`SurfaceError::NoComponent`] for a name `ir` does not declare,
    /// [`SurfaceError::Collision`] when two components claim one route, and
    /// [`SurfaceError::Unsupported`] for a binding that answers a standard refusal its own way
    /// for every operation, a path that is not a template, or a view this ingress cannot route
    /// (one answered other than as `rows`, a standard refusal the binding answers its own way, a
    /// path variable or a parameter in the query string that is not a scalar): ESS publishes no
    /// `501` for a view, so it is refused rather than answered with a status its contract does
    /// not declare.
    pub fn new(
        ir: &EssIr,
        components: &[String],
        binding: Option<&Binding>,
    ) -> Result<Self, SurfaceError> {
        Self::routed(ir, components, binding, None)
    }

    /// [`Surface::new`], every command sent as `caller`: a command that reads its caller is
    /// served rather than marked [`Unserved::Ingress`], and the grant check still runs on each
    /// one. The caller is not checked here; bind it first ([`spindle_kernel::caller_binds`]).
    ///
    /// # Errors
    /// As [`Surface::new`].
    pub fn fixing_caller(
        ir: &EssIr,
        components: &[String],
        binding: Option<&Binding>,
        caller: &Caller,
    ) -> Result<Self, SurfaceError> {
        Self::routed(ir, components, binding, Some(caller))
    }

    fn routed(
        ir: &EssIr,
        components: &[String],
        binding: Option<&Binding>,
        caller: Option<&Caller>,
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
                let route = route(ir, name, &bound, caller)?;
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
    caller: Option<&Caller>,
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
        queried: Vec::new(),
        statuses: BTreeMap::new(),
        input: Vec::new(),
        encoding: Encoding::Json,
        unknown: Unknown::Refuse,
        parameters: Vec::new(),
        responding: Vec::new(),
        unserved: None,
        caller: caller.cloned(),
        not_granted: false,
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
            if let Some(caller) = caller {
                route.not_granted = !may_read(ir, &caller.actor, view);
            }
        }
    }
    Ok(route)
}

/// Whether `actor` may read `view`, by ESS's read grants: a view no actor's `may:` names is open
/// to every actor; a read-granted one only to an actor whose `may:` names it
/// ([`ess_compiler::ir::ResolvedActor::may_read`]).
fn may_read(ir: &EssIr, actor: &str, view: &str) -> bool {
    let names = |declared: &ess_compiler::ir::ResolvedActor| {
        declared
            .may_read
            .iter()
            .any(|granted| granted.name().to_string() == view)
    };
    if !ir.actors().values().any(names) {
        return true;
    }
    ir.actors()
        .iter()
        .any(|(name, declared)| name.to_string() == actor && names(declared))
}

/// Fills `route`'s variables, input, request and statuses; `Err` names what it uses that this
/// ingress does not serve: an outcome answered other than with the envelope (a body, headers, a
/// redirect or text), a view answered other than as `rows`, a standard
/// refusal the binding answers its own way, a command whose outcome answers with its retained
/// result, a path variable or a view parameter in the query string that is not a scalar, or a
/// command that reads its caller where the
/// route fixes no caller: how an HTTP caller proves its attributes is an authentication technical
/// not yet decided (design §11).
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
            if !read.is_empty() && route.caller.is_none() {
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
            let request = ess_gen::http::request(bound);
            route.encoding = request.encoding;
            route.unknown = request.unknown;
            if request.encoding != Encoding::Json {
                let carried = ess_gen::http::carried(ir, bound);
                route.parameters = ir
                    .command(handle)
                    .input
                    .iter()
                    .map(|field| {
                        let (shape, flat) = ess_gen::http::flat(ir, &field.type_ref)
                            .unwrap_or((Shape::One, Flat::Text));
                        Parameter {
                            wire: ess_gen::schema::wire_field_name(field).to_owned(),
                            field: field.name.clone(),
                            flat,
                            shape,
                            carried: carried.contains(field.name.as_str()),
                        }
                    })
                    .collect();
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
        ess_gen::http::Served::View(handle) => {
            if bound
                .declared
                .is_some_and(|declared| declared.body != ViewBody::Rows)
            {
                return Err("a view answered other than as rows".to_owned());
            }
            for param in &ir.view(handle).params {
                if route
                    .variables
                    .iter()
                    .any(|variable| variable.field == param.name)
                {
                    continue;
                }
                let flat = match ess_gen::http::flat(ir, &param.type_ref) {
                    Some((Shape::One | Shape::Optional, flat)) => flat,
                    _ => {
                        return Err(format!(
                            "the parameter {}, which no single query value carries",
                            param.name
                        ));
                    }
                };
                route.queried.push(Queried {
                    field: param.name.clone(),
                    wire: ess_gen::schema::wire_field_name(param).to_owned(),
                    flat,
                });
            }
        }
    }
    Ok(())
}

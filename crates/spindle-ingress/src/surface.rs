//! The routes one listener serves, read from ESS: the `ess-http/1` binding where one is given,
//! a fallback under `/api/` where none is.

use std::collections::BTreeMap;
use std::fmt;

use ess_compiler::EssIr;
use ess_gen::http::{Binding, Flat};
use ess_http::{Body, Encoding, Method, Operation, ResponseEncoding, Segment, ViewBody};
use serde_json::{Map, Value};

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
    /// The path template: the binding's, ESS's derived one for an operation a binding does not
    /// name, or the fallback `/api/commands/<qualified name>` or `/api/views/<qualified name>`
    /// without a binding.
    pub path: String,
    /// The command or view it serves.
    pub operation: Operation,
    segments: Vec<Segment>,
    variables: Vec<Variable>,
    statuses: BTreeMap<String, u16>,
}

impl Route {
    /// The status the command's outcome `outcome` answers: the binding's where it lists one, ESS's
    /// derived status otherwise.
    pub fn status(&self, outcome: &str) -> Option<u16> {
        self.statuses.get(outcome).copied()
    }

    /// The path's variables.
    pub fn variables(&self) -> &[Variable] {
        &self.variables
    }

    /// The input `path` carries in its variables, read as [`ess_gen::http::Flat`] says; `None`
    /// when `path` does not match the template, `Some(Err)` when a variable's text is malformed.
    fn carried(&self, path: &str) -> Option<Result<Map<String, Value>, String>> {
        if !ess_gen::http::template_matches(&self.segments, path) {
            return None;
        }
        let given: Vec<&str> = path.trim_start_matches('/').split('/').collect();
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

/// A path segment with every `%XX` decoded; `None` when the result is not UTF-8 or an escape is
/// broken.
fn decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
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
        Flat::Integer => Some(text.parse::<i64>().map_or(Value::String(text), Value::from)),
        Flat::Boolean => Some(match text.as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => Value::String(text),
        }),
        Flat::Text => Some(Value::String(text)),
    }
}

/// What a request's method and path resolve to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved<'a> {
    /// The route the request is for, and the input its path carries.
    Found(&'a Route, Map<String, Value>),
    /// The path matches a route whose variable it holds malformed.
    Malformed(String),
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
}

impl Surface {
    /// The surface of `components`: for each, the operations ESS derives for it, its accepted
    /// commands and, where it is `reached_by: network`, the views of the domains it owns.
    ///
    /// With `binding`, each operation is served where [`ess_gen::http::bound_routes`] puts it:
    /// at the method and path the binding declares, its path variables read into the input, and
    /// each outcome answered with the status the binding lists ([`ess_gen::http::listed`]) or ESS
    /// derives. Without one, a command is a `POST` at `/api/commands/<qualified name>` and a view a
    /// `GET` at `/api/views/<qualified name>`, each outcome at ESS's derived status.
    ///
    /// # Errors
    /// [`SurfaceError::NoComponent`] for a name `ir` does not declare,
    /// [`SurfaceError::Collision`] when two components claim one route, and
    /// [`SurfaceError::Unsupported`] for a route that needs what this ingress does not serve: a
    /// `form` or `query` request, an outcome answered other than with the envelope (a body,
    /// headers, a redirect or text), a view answered other than as `rows`, a standard refusal the
    /// binding answers its own way, or a command whose outcome answers with its response.
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
                let route = route(ir, name, &bound, binding.is_some())?;
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
        Ok(Self { routes })
    }

    /// Every route, component by component.
    pub fn routes(&self) -> &[Route] {
        &self.routes
    }

    /// The route a request with `method` (any case) and `path` is for. Compiling the binding
    /// refused two templates a request path can both match (`ESS-HTTP-010`, `ESS-HTTP-011`), so
    /// at most one route of a method matches.
    pub fn resolve(&self, method: &str, path: &str) -> Resolved<'_> {
        let mut served = Vec::new();
        for route in &self.routes {
            let Some(carried) = route.carried(path) else {
                continue;
            };
            if !route.method.as_str().eq_ignore_ascii_case(method) {
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

/// Where `operation` is served without a binding: `/api/commands/<qualified name>` or
/// `/api/views/<qualified name>`.
pub fn fallback(operation: &Operation) -> String {
    match operation {
        Operation::Command(name) => format!("/api/commands/{name}"),
        Operation::View(name) => format!("/api/views/{name}"),
    }
}

fn unsupported(operation: &Operation, what: impl Into<String>) -> SurfaceError {
    SurfaceError::Unsupported {
        operation: operation.to_string(),
        what: what.into(),
    }
}

/// One route of `component`, owned.
fn route(
    ir: &EssIr,
    component: &str,
    bound: &ess_gen::http::Route<'_>,
    with_binding: bool,
) -> Result<Route, SurfaceError> {
    let operation = bound.serves.operation();
    let path = if with_binding {
        bound.path.clone()
    } else {
        fallback(&operation)
    };
    let segments = ess_http::template(&path)
        .map_err(|reasons| unsupported(&operation, format!("the path {path}: {reasons:?}")))?;
    let mut variables = Vec::new();
    for (variable, field) in ess_gen::http::path_fields(ir, bound) {
        let position = segments
            .iter()
            .position(|segment| *segment == Segment::Variable(variable.clone()))
            .ok_or_else(|| unsupported(&operation, format!("the variable {variable}")))?;
        let flat = ess_gen::http::scalar(ir, &field.type_ref).ok_or_else(|| {
            unsupported(
                &operation,
                format!("{} in the path, which is not a scalar", field.name),
            )
        })?;
        variables.push(Variable {
            position,
            field: field.name.clone(),
            flat,
        });
    }
    if let Some(declared) = bound.declared
        && let Some(key) = declared.refusals.keys().next()
    {
        return Err(unsupported(
            &operation,
            format!("its own answer to the standard refusal {key}"),
        ));
    }
    let mut statuses = BTreeMap::new();
    match bound.serves {
        ess_gen::http::Served::Command(handle) => {
            if ess_gen::http::request(bound).encoding != Encoding::Json {
                return Err(unsupported(&operation, "a request that is not a JSON body"));
            }
            for outcome in &ir.command(handle).outcomes {
                if outcome.retains_result || ess_http::status::answers_with_response(ir, outcome) {
                    return Err(unsupported(
                        &operation,
                        format!(
                            "{}, which answers with the command's response",
                            outcome.name
                        ),
                    ));
                }
                let status = match ess_gen::http::listed(bound, outcome) {
                    Some(listed) => {
                        if listed.body != Body::Envelope
                            || listed.encoding != ResponseEncoding::default()
                            || !listed.headers.is_empty()
                            || listed.redirect.is_some()
                        {
                            return Err(unsupported(
                                &operation,
                                format!(
                                    "{} answered other than with its status and the envelope",
                                    outcome.name
                                ),
                            ));
                        }
                        listed.status
                    }
                    None => ess_http::status::outcome_status(ir, outcome)
                        .parse()
                        .map_err(|_| unsupported(&operation, "an outcome without a status"))?,
                };
                statuses.insert(outcome.name.to_string(), status);
            }
        }
        ess_gen::http::Served::View(_) => {
            if bound
                .declared
                .is_some_and(|declared| declared.body != ViewBody::Rows)
            {
                return Err(unsupported(
                    &operation,
                    "a view answered other than as rows",
                ));
            }
        }
    }
    Ok(Route {
        component: component.to_owned(),
        method: bound.method.binding(),
        path,
        operation,
        segments,
        variables,
        statuses,
    })
}

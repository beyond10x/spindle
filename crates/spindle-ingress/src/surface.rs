//! The routes one listener serves, read from ESS: the routes `ess generate --kind openapi`
//! publishes, with an `ess-http/1` binding's in place of those it names.

use std::collections::BTreeMap;
use std::fmt;

use ess_compiler::EssIr;
use ess_gen::http::{Binding, Flat, Shape};
use ess_http::{Body, Encoding, Method, Operation, ResponseEncoding, Segment, Unknown, ViewBody};
use serde_json::{Map, Value};

use crate::console::Console;
use crate::observe::topology::Routed;
use crate::observe::{Recorder, Reserved};

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

/// One parameter of a view read from the query string: the parameter, its wire name, how its
/// text is read and whether the key repeats once per element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Queried {
    /// The view parameter it carries.
    pub field: String,
    /// The query key it is read under: the parameter's wire name.
    pub wire: String,
    /// How its text is read ([`ess_gen::http::scalar`]): the value's, or each element's for a
    /// list.
    pub flat: Flat,
    /// Whether the parameter is a `List<T>`, `Optional` or not, which ESS publishes as the key
    /// repeated once per element (`style: form`, `explode: true`, beyond10x/ess#438).
    pub list: bool,
    /// Whether the value, or each element, is a `Decimal` (through newtypes): its text is read as
    /// the exact decimal number it spells ([`Queried::value`]).
    pub decimal: bool,
}

impl Queried {
    /// One value, or one element of a list, read from its decoded `text`: as [`read`] reads it,
    /// except that decimal text for a `Decimal` is the number it spells
    /// ([`ess_primitives::facts::Number::decimal_literal`], as the kernel reads a decimal
    /// literal), the value ESS's interpreter and the host's parameter check take. Text that is not
    /// a decimal literal stays a string, for the host's check to refuse naming the parameter.
    /// `None` for empty text.
    fn value(&self, text: String) -> Option<Value> {
        if self.decimal
            && let Some(number) = ess_primitives::facts::Number::decimal_literal(&text)
                .and_then(|number| serde_json::to_value(number).ok())
        {
            return Some(number);
        }
        read(self.flat, text)
    }
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
    /// Whether an `ess-http/1` binding declares it: a declared template's literals are compared
    /// with a request segment in its [`literal_spelling`]; a derived route's path exactly.
    declared: bool,
    variables: Vec<Variable>,
    queried: Vec<Queried>,
    statuses: BTreeMap<String, u16>,
    input: Vec<String>,
    encoding: Encoding,
    unknown: Unknown,
    parameters: Vec<Parameter>,
    /// The outcomes answered with the command's response beside the envelope.
    responding: Vec<String>,
    /// The outcomes the binding answers with a body other than the envelope, each with it:
    /// `response`, `response.<field>`, `{<member>: error}` or `none`.
    bodies: BTreeMap<String, Body>,
    unserved: Option<Unserved>,
    /// The command's outcomes not run, in declaration order, each with why.
    refused: Vec<(String, Unserved)>,
    caller: Option<Caller>,
    not_granted: bool,
    /// Whether each request proves its own caller ([`Surface::proving_callers`]).
    proves: bool,
}

/// An actor and caller a surface sends commands as: fixed for the whole run
/// ([`Surface::fixing_caller`], [`Surface::fixing_callers`]), a test-backend switch, not
/// authentication; or proven by each request in the header a trusted proxy sets
/// ([`Surface::proving_callers`], [`Caller::proven`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    /// The qualified name of the actor.
    pub actor: String,
    /// The caller's attributes, by declared attribute name.
    pub attributes: Map<String, Value>,
}

impl Caller {
    /// The caller a header `value` proves, as [`Surface::proving_callers`] reads it: one JSON
    /// object, `{"actor": "<qualified actor>", "caller": {"<attribute>": <value>}}`, `caller` left
    /// out for an actor with no attribute. `None` for any other value: not UTF-8 JSON, not an
    /// object, an object at any depth naming a member twice under any spelling of its name
    /// ([`unique_members`]), `actor` missing or not a string, `caller` not an object, or any other
    /// member. The caller is not bound here against a model; [`crate::answer`] binds it before it
    /// is used.
    pub fn proven(value: &[u8]) -> Option<Self> {
        let Ok(Value::Object(mut proof)) = serde_json::from_slice::<Value>(value) else {
            return None;
        };
        if !unique_members(std::str::from_utf8(value).ok()?) {
            return None;
        }
        let Some(Value::String(actor)) = proof.remove("actor") else {
            return None;
        };
        let attributes = match proof.remove("caller") {
            None => Map::new(),
            Some(Value::Object(attributes)) => attributes,
            Some(_) => return None,
        };
        proof.is_empty().then_some(Self { actor, attributes })
    }

    /// Whether `ir` declares this caller's actor and binds its attributes as the kernel binds a
    /// caller ([`spindle_kernel::caller_binds`]).
    pub(crate) fn binds(&self, ir: &EssIr) -> bool {
        ir.actors()
            .keys()
            .any(|name| name.to_string() == self.actor)
            && spindle_kernel::caller_binds(ir, Some(&self.actor), Some(&self.attributes)).is_ok()
    }
}

/// Whether every object in `json`, text that parses as JSON, names each member once: each name
/// is compared once its escapes are decoded, so `"actor"` and `"actor"` are the same member.
/// A parser keeps one of two same-named members, and which one differs between parsers, so a
/// proxy and this ingress could read two different callers from one value.
fn unique_members(json: &str) -> bool {
    // Per open object, the names seen and whether a name comes next; `None` for an array.
    let mut open: Vec<Option<(std::collections::BTreeSet<String>, bool)>> = Vec::new();
    let bytes = json.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'{' => open.push(Some((std::collections::BTreeSet::new(), true))),
            b'[' => open.push(None),
            b'}' | b']' => {
                open.pop();
            }
            b',' => {
                if let Some(Some((_, name_next))) = open.last_mut() {
                    *name_next = true;
                }
            }
            b':' => {
                if let Some(Some((_, name_next))) = open.last_mut() {
                    *name_next = false;
                }
            }
            b'"' => {
                let start = at;
                at += 1;
                while at < bytes.len() && bytes[at] != b'"' {
                    at += if bytes[at] == b'\\' { 2 } else { 1 };
                }
                if let Some(Some((seen, true))) = open.last_mut() {
                    let Some(Ok(name)) = json.get(start..=at).map(serde_json::from_str::<String>)
                    else {
                        return false;
                    };
                    if !seen.insert(name) {
                        return false;
                    }
                }
            }
            _ => {}
        }
        at += 1;
    }
    true
}

/// Why a route is kept but its operation not served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unserved {
    /// The command's or view's route uses what this ingress does not serve, named. A request for
    /// it is answered `501` before anything is read or written.
    Ingress(String),
    /// The kernel does not run the command or the outcome, as its text: for a command, one it
    /// cannot select among the outcomes of ([`selects`](spindle_kernel::selects)) or none of whose
    /// outcomes it takes ([`takes`](spindle_kernel::takes)); a request for it is answered as
    /// before this was known: the host refuses it before anything is read or written, `501`
    /// naming the construct. For an outcome ([`Route::refused`]), one it does not take.
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

    /// The command's outcomes the route does not run, in declaration order, each with why:
    /// [`Unserved::Ingress`] for one this ingress does not answer as the binding lists it,
    /// [`Unserved::Kernel`] for one the kernel does not take ([`spindle_kernel::takes`]). The
    /// route is served: a request whose selected outcome is one of these answers `501` naming it,
    /// and nothing is written; every other outcome answers as declared. Empty for a view and for
    /// an operation not served at all ([`Route::unserved`]).
    pub fn refused(&self) -> &[(String, Unserved)] {
        &self.refused
    }

    /// What this ingress does not serve of each outcome it refuses, by outcome name: the kernel
    /// refuses one where it is selected, before anything is written
    /// ([`spindle_kernel::execute_refusing`]).
    pub(crate) fn refusing(&self) -> BTreeMap<String, String> {
        self.refused
            .iter()
            .filter(|(_, why)| matches!(why, Unserved::Ingress(_)))
            .map(|(outcome, why)| (outcome.clone(), why.to_string()))
            .collect()
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

    /// The body the binding answers the command's outcome `outcome` with where it is not the
    /// envelope: [`Body::Response`], [`Body::ResponseField`], [`Body::Error`] or [`Body::None`].
    /// `None` for an outcome answered with the envelope.
    pub(crate) fn body(&self, outcome: &str) -> Option<&Body> {
        self.bodies.get(outcome)
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
    /// as [`ess_gen::http::Flat`] says, a `Decimal`'s decimal text as the number it spells. A
    /// `List<T>` parameter is read as ESS publishes it, the key
    /// repeated once per element (`tags=a&tags=b`), each element read as `T` in the order given;
    /// `tags=a,b` is the one element `a,b`. A parameter the query leaves out is left out, for the
    /// host to refuse where it is required.
    ///
    /// # Errors
    /// Naming the parameter, under `query.`: a key that is not a list's given more than once, a
    /// value that is not percent-encoded UTF-8, an empty value for a parameter that is not a
    /// `String`, or an empty element of a list of any type.
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
            let text = |raw: &str| {
                form_decoded(raw).ok_or_else(|| {
                    format!("{place}: expected percent-encoded UTF-8 text, found `{raw}`")
                })
            };
            if queried.list {
                if found.is_empty() {
                    continue;
                }
                let elements = found
                    .into_iter()
                    .map(|raw| {
                        let element = text(raw)?;
                        if element.is_empty() {
                            return Err(format!("{place}: expected an element, found empty text"));
                        }
                        queried.value(element).ok_or_else(|| {
                            format!("{place}: expected an element, found empty text")
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                read.insert(queried.field.clone(), Value::Array(elements));
                continue;
            }
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
            let value = queried
                .value(text(raw)?)
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

    /// The fixed caller this route's operation is sent or read as: the first whose actor its
    /// grant names, else the first fixed; `None` where the surface fixes none.
    pub fn caller(&self) -> Option<&Caller> {
        self.caller.as_ref()
    }

    /// Whether the route's view is read-granted (ess/22 `may:` naming a view, `x-ess-may-read`)
    /// and no fixed caller's actor may read it: ESS's served surface then refuses it `403`
    /// "not granted" before reading anything. Always `false` without a fixed caller.
    pub fn not_granted(&self) -> bool {
        self.not_granted
    }

    /// Whether each request for this route proves its own caller ([`Surface::proving_callers`]):
    /// one that proves none is refused, and the route fixes no caller.
    pub fn proves(&self) -> bool {
        self.proves
    }

    /// The input `path` carries in its variables, read as [`ess_gen::http::Flat`] says; `None`
    /// when `path` does not match the template or leaves a variable's segment empty (ESS's
    /// served application matches only a variable holding a segment that is not empty),
    /// `Some(Err)` when a variable's text is malformed. A declared template's literals are
    /// compared with `path` in its [`literal_spelling`], as ESS's served application compares
    /// them, so `/things/%7Emodules` matches `/things/~modules`; a variable is read from the raw
    /// segment, every escape decoded ([`decoded`]). Nothing is read from a matched path for an
    /// operation the ingress does not serve or a view the fixed caller may not read: both are
    /// refused before their request is read.
    fn carried(&self, path: &str) -> Option<Result<Map<String, Value>, String>> {
        let spelled;
        let compared = if self.declared {
            spelled = literal_spelling(path);
            spelled.as_str()
        } else {
            path
        };
        if !ess_gen::http::template_matches(&self.segments, compared) {
            return None;
        }
        let given: Vec<&str> = path.strip_prefix('/').unwrap_or(path).split('/').collect();
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

/// `text` with each `%XX` escape of an RFC 3986 unreserved character (ALPHA / DIGIT / `-` / `.` /
/// `_` / `~`, §2.3) decoded and every other escape left as written (§6.2.2.2): the spelling ESS
/// 0.90.0's served application compares a request segment with a literal in
/// (`literal_spelling` in the server `ess generate synthesize --http` emits,
/// `docs/design/http-presentation-binding.md`, "Path templates"). ESS publishes it only as
/// emitted source, not as a function this crate can call, so it is written here to the same rule.
/// It decodes no `/`, so `text` keeps its segments.
fn literal_spelling(text: &str) -> String {
    let bytes = text.as_bytes();
    let digit = |at: usize| {
        bytes
            .get(at)
            .and_then(|byte| char::from(*byte).to_digit(16))
    };
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = match (bytes[index], digit(index + 1), digit(index + 2)) {
            (b'%', Some(high), Some(low)) => u8::try_from(high * 16 + low)
                .ok()
                .filter(|byte| byte.is_ascii_alphanumeric() || b"-._~".contains(byte)),
            _ => None,
        };
        if let Some(byte) = escaped {
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_owned())
}

/// A path segment with every `%XX` decoded, as ESS's served application decodes it; `None` where
/// an escape is not two hexadecimal digits or the bytes are not UTF-8. A `+` stays a `+`.
pub(crate) fn decoded(text: &str) -> Option<String> {
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
pub(crate) fn form_decoded(text: &str) -> Option<String> {
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

/// Whether `type_ref`, under `Optional`, `List` and newtypes, is the primitive `Decimal`; the
/// newtype chain is walked as [`ess_gen::http::scalar`] walks it.
fn decimal(ir: &EssIr, type_ref: &ess_compiler::ir::ResolvedTypeRef) -> bool {
    use ess_compiler::ir::{ResolvedBody, ResolvedTypeRef};
    match type_ref {
        ResolvedTypeRef::Primitive { name } => name.as_str() == "Decimal",
        ResolvedTypeRef::Optional { of } | ResolvedTypeRef::List { of } => decimal(ir, of),
        ResolvedTypeRef::Declared { name } => match &ir.named_type(name).body {
            ResolvedBody::Newtype { of, .. } => decimal(ir, of),
            _ => false,
        },
        ResolvedTypeRef::Map { .. } => false,
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
    Malformed(&'a Route, String),
    /// A console route ([`Surface::with_console`]).
    Console(Console),
    /// A path under the reserved prefix `/__spindle/`, matched before any other route, with the
    /// recorder on or off ([`Surface::observing`]).
    Reserved(Reserved),
    /// The path is served, under these methods only: sorted, each once.
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
    /// A route the binding or ESS publishes lies under the reserved prefix `/__spindle/`, which
    /// the run's own routes hold.
    Reserved {
        /// The command or view.
        operation: String,
        /// Its method.
        method: Method,
        /// Its path template.
        path: String,
    },
    /// The route of an operation uses what this ingress does not serve, named.
    Unsupported {
        /// The command or view.
        operation: String,
        /// What it uses.
        what: String,
    },
    /// The header a caller is proven in ([`Surface::proving_callers`]) is no HTTP field name, or
    /// is [`Surface::SECRET_HEADER`].
    Header(String),
    /// The proxy's secret ([`Surface::proving_callers`]) is empty or holds a byte that is no
    /// visible ASCII character, so no header could carry it. The secret is not held here.
    Secret,
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
            Self::Reserved {
                operation,
                method,
                path,
            } => write!(
                formatter,
                "{operation} is served at {} {path}, under /{}/, which this run reserves for its own routes; the binding must serve it at another path",
                method.as_str().to_uppercase(),
                crate::observe::RESERVED_SEGMENT
            ),
            Self::Unsupported { operation, what } => {
                write!(formatter, "{operation}: this ingress does not serve {what}")
            }
            Self::Header(name) => write!(
                formatter,
                "`{name}` is no HTTP header name for the caller: one or more of the token \
                 characters of RFC 9110 §5.6.2, other than {}",
                Surface::SECRET_HEADER
            ),
            Self::Secret => formatter.write_str(
                "the proxy's secret is empty or holds a byte that is no visible ASCII character",
            ),
        }
    }
}

impl std::error::Error for SurfaceError {}

/// Every route of the served components, component by component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Surface {
    routes: Vec<Route>,
    console: bool,
    recorder: Option<Recorder>,
    panel: Option<&'static str>,
    /// The header each request proves its caller in, lowercase ([`Surface::proving_callers`]).
    caller_header: Option<String>,
    /// The secret the proxy presents in [`Surface::SECRET_HEADER`] on each such request.
    caller_secret: Option<Secret>,
}

/// The proxy's secret: never written by `Debug`, so no surface printed for a diagnosis shows it.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Secret(String);

impl Secret {
    /// The secret itself, to compare a request's with; only the listener reads a request's
    /// headers.
    #[cfg_attr(not(feature = "listener"), allow(dead_code))]
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Secret(..)")
    }
}

impl Surface {
    /// The header a proxy presents the secret in on each request to a surface made
    /// [`Surface::proving_callers`]; a request without it, or with another value, proves no
    /// caller.
    pub const SECRET_HEADER: &'static str = "Spindle-Proxy-Secret";

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
    /// A command or view this ingress cannot route keeps its route and is marked
    /// [`Unserved::Ingress`]: for a command, an outcome that answers with the command's retained
    /// result, or the caller attributes it reads where no caller is fixed; for a view, one
    /// answered other than as `rows`, or a parameter in the query string that no single scalar
    /// value carries; for either, a standard refusal the binding answers its own way or a path
    /// variable that is not a scalar. A command the kernel cannot select among the outcomes of
    /// ([`selects`](spindle_kernel::selects)) is marked [`Unserved::Kernel`], and a view the
    /// host's [`reads`](spindle_host::reads) refuses [`Unserved::Host`]. A command's outcome
    /// answered with headers, a redirect or the `text` encoding, or one the kernel does not take
    /// ([`takes`](spindle_kernel::takes)) refuses that outcome alone ([`Route::refused`]); a
    /// command none of whose outcomes is left is marked as a whole.
    ///
    /// # Errors
    /// [`SurfaceError::NoComponent`] for a name `ir` does not declare,
    /// [`SurfaceError::Collision`] when two components claim one route,
    /// [`SurfaceError::Reserved`] for a route whose first segment is `__spindle`, and
    /// [`SurfaceError::Unsupported`] for a binding that answers a standard refusal its own way
    /// for every operation, or a path that is not a template.
    pub fn new(
        ir: &EssIr,
        components: &[String],
        binding: Option<&Binding>,
    ) -> Result<Self, SurfaceError> {
        Self::routed(ir, components, binding, &[], false)
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
        Self::routed(ir, components, binding, std::slice::from_ref(caller), false)
    }

    /// [`Surface::new`], each command sent as the first of `callers` whose actor's `may:` names
    /// it, and each read-granted view read as the first that may read it ([`Route::caller`]). An
    /// operation none of them may run is sent as the first, so it answers ESS's standard refusal
    /// naming that actor and writes nothing. A command that reads its caller is served rather than
    /// marked [`Unserved::Ingress`]. With no callers this is [`Surface::new`]. The callers are not
    /// checked here; bind each first ([`spindle_kernel::caller_binds`]).
    ///
    /// # Errors
    /// As [`Surface::new`].
    pub fn fixing_callers(
        ir: &EssIr,
        components: &[String],
        binding: Option<&Binding>,
        callers: &[Caller],
    ) -> Result<Self, SurfaceError> {
        Self::routed(ir, components, binding, callers, false)
    }

    /// [`Surface::new`], each request for a command or view proving its own caller in the header
    /// `header` ([`Caller::proven`]), which a proxy in front of the listener sets once it has
    /// authenticated the request, beside `secret` in [`Surface::SECRET_HEADER`], by which the
    /// proxy is trusted: the listener reads both, and a command that reads its caller is served
    /// rather than marked [`Unserved::Ingress`]. Each request's caller is bound against the
    /// model before it is used, and the grant check runs as for a fixed caller: a command its
    /// actor's `may:` does not name, or a read-granted view it may not read, answers ESS's standard
    /// `403` naming that actor. A request without the header, with it more than once, or with a
    /// value that proves no caller of the model, and one whose secret is missing, given more than
    /// once or another (compared in time that depends only on the lengths), answers the standard
    /// refusal for a request authenticated as no caller, `403`
    /// `{"refused": "not granted", "actor": null}`, and nothing is read or written; no default
    /// caller is taken. The console routes and those under `/__spindle/` serve operator tooling
    /// behind the proxy: each answers a request that carries the secret, with or without a caller,
    /// and refuses one without it the same way, doing nothing. A request [`crate::route`] answers
    /// carries no header, so there every request but one for a path no route has is refused.
    ///
    /// The headers are trusted only because the proxy sets them: the listener must be reachable
    /// only through the proxy, and the proxy must remove both from every request it forwards
    /// before setting its own.
    ///
    /// # Errors
    /// As [`Surface::new`]; [`SurfaceError::Header`] for a `header` that is no HTTP field name or
    /// is [`Surface::SECRET_HEADER`], and [`SurfaceError::Secret`] for a `secret` that is empty or
    /// holds a byte that is no visible ASCII character.
    pub fn proving_callers(
        ir: &EssIr,
        components: &[String],
        binding: Option<&Binding>,
        header: &str,
        secret: &str,
    ) -> Result<Self, SurfaceError> {
        if header.is_empty()
            || !header.bytes().all(token)
            || header.eq_ignore_ascii_case(Self::SECRET_HEADER)
        {
            return Err(SurfaceError::Header(header.to_owned()));
        }
        if secret.is_empty() || !secret.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err(SurfaceError::Secret);
        }
        let mut surface = Self::routed(ir, components, binding, &[], true)?;
        surface.caller_header = Some(header.to_ascii_lowercase());
        surface.caller_secret = Some(Secret(secret.to_owned()));
        Ok(surface)
    }

    /// The secret the proxy presents on each request, where the surface was made
    /// [`Surface::proving_callers`]; only the listener reads a request's headers.
    #[cfg_attr(not(feature = "listener"), allow(dead_code))]
    pub(crate) fn caller_secret(&self) -> Option<&Secret> {
        self.caller_secret.as_ref()
    }

    /// The header each request proves its caller in, lowercase, where the surface was made
    /// [`Surface::proving_callers`].
    pub fn caller_header(&self) -> Option<&str> {
        self.caller_header.as_deref()
    }

    fn routed(
        ir: &EssIr,
        components: &[String],
        binding: Option<&Binding>,
        callers: &[Caller],
        proves: bool,
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
                let route = route(ir, name, &bound, callers, proves)?;
                if Reserved::of(&route.path).is_some() {
                    return Err(SurfaceError::Reserved {
                        operation: route.operation.to_string(),
                        method: route.method,
                        path: route.path,
                    });
                }
                if let Some(taken) = routes.iter().find(|taken| {
                    taken.method == route.method && same_template(&taken.segments, &route.segments)
                }) {
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
            recorder: None,
            panel: None,
            caller_header: None,
            caller_secret: None,
        })
    }

    /// This surface recording the requests its listener answers, the last `capacity` of them
    /// (held to 1 through [`REQUESTS_MOST`](crate::REQUESTS_MOST)) and the totals per
    /// [`Class`](crate::Class), and answering `GET /__spindle/requests` with them. Clones share
    /// the one recorder. The recorder holds every route, each a `routes` edge of
    /// `GET /__spindle/topology`, also one answered `501`.
    #[must_use]
    pub fn observing(mut self, capacity: usize) -> Self {
        let routes = self
            .routes
            .iter()
            .map(|route| Routed {
                method: route.method.as_str().to_uppercase(),
                path: route.path.clone(),
                operation: route.operation.name().to_string(),
                kind: match route.operation {
                    Operation::Command(_) => "command",
                    Operation::View(_) => "view",
                },
            })
            .collect();
        self.recorder = Some(Recorder::new(capacity).routing(routes));
        self
    }

    /// The recorder, where the surface was made [`Surface::observing`]; `None` otherwise.
    pub fn recorder(&self) -> Option<&Recorder> {
        self.recorder.as_ref()
    }

    /// This surface answering `GET /__spindle/ui`, where it is [`Surface::observing`], with
    /// `page`: the observe panel, one HTML page that loads nothing beside itself, answered with
    /// [`PANEL_POLICY`](crate::PANEL_POLICY). Without one an observing surface answers that
    /// route `503`.
    #[must_use]
    pub fn with_panel(mut self, page: &'static str) -> Self {
        self.panel = Some(page);
        self
    }

    /// The panel page, where the surface was given one ([`Surface::with_panel`]).
    pub fn panel(&self) -> Option<&'static str> {
        self.panel
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
    /// Compiling the binding refuses two templates of one method that are the same but for
    /// variable names (`ESS-HTTP-010`) or that cross (`ESS-HTTP-011`); from ESS 0.89.0 it admits a
    /// nested pair, where one template holds the literal at every position the two differ. Of the
    /// routes of `method` that match, the one ESS's dispatch order tries first answers
    /// ([`ess_gen::http::dispatch_order`]): the more specific of a nested pair, whatever order the
    /// routes are declared or held in, and otherwise the first. A path no route of `method`
    /// matches answers the methods of the routes that do, sorted and each once, as ESS's served
    /// application names them in `Allow`. A console route, where the surface has
    /// them, is matched exactly; no other route shares its path ([`Surface::with_console`]). A
    /// path under the reserved prefix `/__spindle/` is matched before either, with or without a
    /// recorder, so that it never reaches a route of the specification.
    pub fn resolve(&self, method: &str, path: &str) -> Resolved<'_> {
        if let Some(reserved) = Reserved::of(path) {
            return Resolved::Reserved(reserved);
        }
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
        let mut matched = Vec::new();
        for route in &self.routes {
            let Some(carried) = route.carried(path) else {
                continue;
            };
            if route.method.as_str().to_uppercase() == method {
                matched.push((route, carried));
            } else {
                served.push(route.method);
            }
        }
        let rows: Vec<(&str, &str)> = matched
            .iter()
            .map(|(route, _)| (route.method.as_str(), route.path.as_str()))
            .collect();
        if let Some(&first) = ess_gen::http::dispatch_order(&rows).first() {
            let (route, carried) = matched.swap_remove(first);
            return match carried {
                Ok(carried) => Resolved::Found(route, carried),
                Err(why) => Resolved::Malformed(route, why),
            };
        }
        if served.is_empty() {
            Resolved::NotFound
        } else {
            // As ESS's served application names them in `Allow`: sorted, each method once.
            served.sort_unstable_by_key(|method| method.as_str());
            served.dedup();
            Resolved::WrongMethod(served)
        }
    }
}

/// Whether two templates name one route under ESS's rule (ESS-HTTP-010): the same segments, the
/// variable names erased.
fn same_template(left: &[Segment], right: &[Segment]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|pair| match pair {
            (Segment::Variable(_), Segment::Variable(_)) => true,
            (left, right) => left == right,
        })
}

/// Whether `byte` is a token character (RFC 9110 §5.6.2), what an HTTP field name is made of.
fn token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

/// One route of `component`, owned: served, or kept and marked [`Unserved`] with what it uses
/// that this ingress, the kernel or the host does not run. Where `proves`, each request proves its
/// own caller ([`Route::proves`]).
///
/// # Errors
/// [`SurfaceError::Unsupported`] when the route's path is not a template ESS reads, so that no
/// request can be matched to it.
fn route(
    ir: &EssIr,
    component: &str,
    bound: &ess_gen::http::Route<'_>,
    callers: &[Caller],
    proves: bool,
) -> Result<Route, SurfaceError> {
    let operation = bound.serves.operation();
    let path = bound.path.clone();
    let segments = ess_http::template(&path).map_err(|reasons| SurfaceError::Unsupported {
        operation: operation.to_string(),
        what: format!("the path {path}: {reasons:?}"),
    })?;
    let caller = chosen(ir, &operation, callers);
    let mut route = Route {
        component: component.to_owned(),
        method: bound.method.binding(),
        path,
        operation,
        segments,
        declared: bound.declared.is_some(),
        variables: Vec::new(),
        queried: Vec::new(),
        statuses: BTreeMap::new(),
        input: Vec::new(),
        encoding: Encoding::Json,
        unknown: Unknown::Refuse,
        parameters: Vec::new(),
        responding: Vec::new(),
        bodies: BTreeMap::new(),
        unserved: None,
        refused: Vec::new(),
        caller: caller.cloned(),
        not_granted: false,
        proves,
    };
    match (served(ir, bound, &mut route), &route.operation) {
        (Err(what), _) => {
            route.variables.clear();
            route.queried.clear();
            route.statuses.clear();
            route.responding.clear();
            route.bodies.clear();
            route.refused.clear();
            route.unserved = Some(Unserved::Ingress(what));
        }
        (Ok(()), Operation::Command(_)) => {
            if let ess_gen::http::Served::Command(handle) = bound.serves {
                refuse(ir, ir.command(handle), &mut route);
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

/// The fixed caller `operation` is sent or read as: the first of `callers` whose actor may run the
/// command or read the view, else the first, whom the grant check then refuses; `None` without
/// callers. A view no actor's grant names is open, so the first may read it.
fn chosen<'a>(ir: &EssIr, operation: &Operation, callers: &'a [Caller]) -> Option<&'a Caller> {
    let granted = |caller: &&Caller| match operation {
        Operation::Command(command) => may_run(ir, &caller.actor, command),
        Operation::View(view) => may_read(ir, &caller.actor, view),
    };
    callers.iter().find(granted).or_else(|| callers.first())
}

/// Whether `actor` may send `command`: a declared actor whose `may:` names it
/// ([`ess_compiler::ir::ResolvedActor::may`]), the grant the kernel checks before it runs one.
fn may_run(ir: &EssIr, actor: &str, command: &str) -> bool {
    ir.actors().iter().any(|(name, declared)| {
        name.to_string() == actor
            && declared
                .may
                .iter()
                .any(|granted| granted.name().to_string() == command)
    })
}

/// Whether `actor` may read `view`, by ESS's read grants: a view no actor's `may:` names is open
/// to every actor; a read-granted one only to an actor whose `may:` names it
/// ([`ess_compiler::ir::ResolvedActor::may_read`]).
pub(crate) fn may_read(ir: &EssIr, actor: &str, view: &str) -> bool {
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

/// Marks what the route of `command`, once [`served`] filled it, does not run. Each outcome the
/// kernel does not take ([`spindle_kernel::takes`]) is refused beside those this ingress does not
/// answer, in declaration order. Where the kernel cannot select among the outcomes
/// ([`spindle_kernel::selects`]) or none is left to serve, the whole route is marked instead: by
/// what this ingress does not serve, first, which is answered before the request is read, else
/// by what the kernel does not run.
fn refuse(ir: &EssIr, command: &ess_compiler::ir::ResolvedCommand, route: &mut Route) {
    let mut refused = Vec::new();
    for outcome in &command.outcomes {
        let name = outcome.name.to_string();
        if let Some(at) = route.refused.iter().position(|(listed, _)| *listed == name) {
            refused.push(route.refused.remove(at));
        } else if let Err(why) = spindle_kernel::takes(ir, command, outcome) {
            refused.push((name, Unserved::Kernel(why.to_string())));
        }
    }
    let selection = spindle_kernel::selects(command).err();
    let none_left = !command.outcomes.is_empty() && refused.len() == command.outcomes.len();
    if selection.is_none() && !none_left {
        route.refused = refused;
        return;
    }
    let ingress = refused
        .iter()
        .find(|(_, why)| matches!(why, Unserved::Ingress(_)))
        .map(|(_, why)| why.clone());
    if let Some(why) = ingress {
        route.variables.clear();
        route.statuses.clear();
        route.responding.clear();
        route.bodies.clear();
        route.unserved = Some(why);
    } else {
        route.unserved = selection
            .map(|refused| Unserved::Kernel(refused.to_string()))
            .or_else(|| refused.into_iter().next().map(|(_, why)| why));
    }
}

/// Fills `route`'s variables, input, request, statuses and the bodies its outcomes answer other
/// than with the envelope, and lists among its refused outcomes each one answered with headers, a
/// redirect or the `text` encoding ([`unwritten`]), or
/// without a status; `Err` names what it uses that this ingress does not serve: a view answered
/// other than as `rows`, a standard
/// refusal the binding answers its own way, a command whose outcome answers with its retained
/// result, a path variable or a view parameter in the query string that is not a scalar, or a
/// command that reads its caller where the
/// route neither fixes a caller nor has each request prove one ([`Surface::proving_callers`],
/// design §11).
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
            if !read.is_empty() && route.caller.is_none() && !route.proves {
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
                    Some(listed) => match unwritten(listed) {
                        Some(what) => Err(format!("{} answered {what}", outcome.name)),
                        None => {
                            if listed.body != Body::Envelope {
                                route
                                    .bodies
                                    .insert(outcome.name.to_string(), listed.body.clone());
                            }
                            Ok(listed.status)
                        }
                    },
                    None => ess_http::status::outcome_status(ir, outcome)
                        .parse()
                        .map_err(|_| format!("{} without a status", outcome.name)),
                };
                match status {
                    Ok(status) => {
                        route.statuses.insert(outcome.name.to_string(), status);
                    }
                    Err(what) => route
                        .refused
                        .push((outcome.name.to_string(), Unserved::Ingress(what))),
                }
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
                // ESS 0.88.0: `ess_gen::http::flat` reads no `Optional<List<T>>`, which
                // `openapi::view_parameters` publishes as a list (`is_list`); the `Optional` is
                // stripped first, and the host checks presence against the declared type.
                let (list, flat) = match ess_gen::http::flat(ir, param.type_ref.required()) {
                    Some((Shape::List, flat)) => (true, flat),
                    Some((Shape::One | Shape::Optional, flat)) => (false, flat),
                    None => {
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
                    list,
                    decimal: decimal(ir, &param.type_ref),
                });
            }
        }
    }
    Ok(())
}

/// What of `listed`, a command outcome's answer as the binding lists it, this ingress does not
/// write, in words; `None` where it writes all of it: the status with the envelope,
/// `response`, `response.<field>`, the error object `{<member>: error}` or `none` as
/// `application/json` or no body. Each of bound headers, a redirect and a response encoding other
/// than `json` is named.
///
/// Every member of the answer is read by name, so a member a later ESS adds fails to compile here
/// rather than going unwritten unnamed.
fn unwritten(listed: &ess_http::Outcome) -> Option<String> {
    let ess_http::Outcome {
        status: _,
        body,
        encoding,
        headers,
        redirect,
    } = listed;
    let mut what = Vec::new();
    match body {
        Body::Envelope
        | Body::Response
        | Body::ResponseField(_)
        | Body::None
        | Body::Error { .. } => {}
    }
    match encoding {
        ResponseEncoding::Json => {}
        ResponseEncoding::Text => what.push(format!("in the `{encoding}` encoding")),
    }
    if !headers.is_empty() {
        let names = headers
            .keys()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ");
        what.push(format!("with headers ({names})"));
    }
    if redirect.is_some() {
        what.push("with a redirect".to_owned());
    }
    match what.split_last() {
        None => None,
        Some((last, [])) => Some(last.clone()),
        Some((last, rest)) => Some(format!("{} and {last}", rest.join(", "))),
    }
}

#[cfg(test)]
mod tests {
    use super::{Caller, Flat, Queried, Surface, unique_members};
    use serde_json::{Value, json};

    #[test]
    fn a_member_named_twice_at_any_depth_or_spelling_is_found() {
        for twice in [
            r#"{"actor": "A", "actor": "B"}"#,
            r#"{"actor": "A", "actor": "B"}"#,
            r#"{"actor": "A", "caller": {"id": 1, "id": 2}}"#,
            r#"{"actor": "A", "caller": {"list": [{"x": 1, "x": 2}]}}"#,
        ] {
            assert!(!unique_members(twice), "{twice}");
            assert_eq!(Caller::proven(twice.as_bytes()), None, "{twice}");
        }
    }

    #[test]
    fn names_in_values_and_in_sibling_objects_are_not_members_named_twice() {
        for once in [
            r#"{"actor": "actor", "caller": {"actor": "\"actor\": {\"a\": 1, \"a\": 2}"}}"#,
            r#"{"actor": "A", "caller": {"rows": [{"id": 1}, {"id": 2}], "id": 3}}"#,
            r#"{"actor": "A", "caller": {"k\"ey": 1, "key": 2}}"#,
        ] {
            assert!(unique_members(once), "{once}");
        }
        assert!(
            Caller::proven(br#"{"actor": "A", "caller": {"actor": "B"}}"#).is_some(),
            "an attribute may share the name actor"
        );
    }

    #[test]
    fn a_surface_without_observe_holds_no_recorder() {
        let ir = spindle_spec::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/desk"),
        )
        .expect("the desk fixture compiles")
        .ir;
        let surface = Surface::new(&ir, &["desk".to_owned()], None).expect("the desk is served");
        assert!(surface.recorder().is_none());
        let console = surface
            .clone()
            .with_console(crate::HTTP_LOOPBACK)
            .expect("the console is added");
        assert!(console.recorder().is_none(), "the console adds no recorder");
        let observed = surface.observing(3);
        assert_eq!(observed.recorder().map(crate::Recorder::capacity), Some(3));
    }

    fn queried(decimal: bool) -> Queried {
        Queried {
            field: "amount".to_owned(),
            wire: "amount".to_owned(),
            flat: Flat::Text,
            list: false,
            decimal,
        }
    }

    #[test]
    fn decimal_text_is_read_as_the_number_it_spells_and_nothing_else_is() {
        let amount = queried(true);
        assert_eq!(amount.value("3.25".to_owned()), Some(json!(3.25)));
        assert_eq!(
            amount.value("-2".to_owned()),
            Some(json!(-2.0)),
            "ESS's `Number` writes a decimal as binary64, as the kernel writes one"
        );
        for text in ["abc", "1e3", ".5", "01", "+1", "3.2.5"] {
            assert_eq!(
                amount.value(text.to_owned()),
                Some(Value::String(text.to_owned())),
                "`{text}` is no decimal literal, so it stays text for the host to refuse"
            );
        }
        assert_eq!(amount.value(String::new()), None, "empty text is malformed");
        assert_eq!(
            queried(false).value("3.25".to_owned()),
            Some(Value::String("3.25".to_owned())),
            "only a `Decimal` is read as a number"
        );
    }
}

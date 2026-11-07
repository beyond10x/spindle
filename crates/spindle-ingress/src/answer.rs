//! One request's answer, from the host that runs the command or reads the view.

use ess_gen::http::{FORBIDDEN, NOT_GRANTED};
use ess_http::{Encoding, Operation};
use serde_json::{Map, Value, json};
use spindle_host::{Failed, Host, HostError};
use spindle_kernel::{DeclaredError, Event, ExecuteError};

use crate::surface::{Route, Unserved};

/// A status and the JSON body written under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// The HTTP status.
    pub status: u16,
    /// The body, written as `application/json`.
    pub body: Value,
}

impl Answer {
    /// A refusal this surface makes rather than the specification, at `status`:
    /// `{"refused": <detail>}`, as ESS's served applications write one.
    pub fn refusal(status: u16, detail: impl Into<String>) -> Self {
        Self {
            status,
            body: json!({"refused": detail.into()}),
        }
    }

    /// ESS's standard refusal for an actor no grant admits, on a command or a read-granted view:
    /// `403` `{"refused": "not granted", "actor": <actor>}`.
    pub fn not_granted(actor: &str) -> Self {
        Self {
            status: FORBIDDEN.parse().unwrap_or(403),
            body: json!({"refused": NOT_GRANTED, "actor": actor}),
        }
    }

    /// The `501` the contract declares on every command: the realization is unfinished.
    /// `committed` says whether the command's effect was written before it stopped.
    pub fn unfinished(detail: impl Into<String>, committed: bool) -> Self {
        Self {
            status: 501,
            body: json!({"refused": detail.into(), "committed": committed}),
        }
    }
}

/// Answers one request for `route`, whose path carries `carried`, whose query string (after the
/// `?`, still percent-encoded) is `query`, whose `Content-Type` is `content_type` and whose body
/// is `body`, by running it on `host`.
///
/// A command reads its input as its route's request declares ([`Route::encoding`]): by
/// default its body as a JSON object, the `{command}.Input` ESS publishes, less the fields the
/// path carries, an empty body the empty input and the `Content-Type` not read; for `form`, an
/// `application/x-www-form-urlencoded` body, any other `Content-Type` refused `400` as ESS's
/// served application refuses it; for `query`, the query string, the body not read. A `form` or
/// `query` value is read by its field's type as a path variable is. A field the command does not
/// declare, or one both the path and the request carry, is refused `400`. It answers the status the
/// route gives the outcome the model selected ([`Route::status`]: the binding's, else ESS's
/// derived one) with the envelope ESS publishes for it: `outcome`, `published` (each event as
/// `{"event", "payload"}`, in emit order), on a refusing outcome `error`, the error's
/// qualified name, with `payload` where the kernel determined its fields, none for the
/// `wrong_state:` answer to an identity no record carries, and on an outcome that
/// returns the command's response (`returns: true`, `200`) `response`, the values the kernel
/// chose for its declared fields.
///
/// A view answers `200` with the body ESS publishes for it, `{"rows": [...]}`, every row it
/// holds for its parameters: those `carried` holds from the path, and those `query` holds
/// ([`Route::parameters`]), read by [`spindle_host::Host::list_with`]. A required parameter left out, one
/// not of its declared type or breaking its newtype, or a malformed query value is refused `400`,
/// naming it. A filter the model leaves unknown for a row is refused `501`, as ESS's interpreter
/// refuses the read.
///
/// A body that is not a JSON object, or input the kernel refuses, is refused `400`. A command the
/// kernel does not run answers `501` with `committed: false`; one whose effect was written and
/// whose consequences could not be carried answers `501` with `committed: true`. A view the host
/// cannot answer is refused `501`, naming what it does not read.
///
/// A command is sent as the route's fixed caller ([`Route::caller`]), else as no caller. One its
/// actor's `may:` does not name answers ESS's standard refusal, `403`
/// `{"refused": "not granted", "actor": <actor>}`, and nothing is written. So does a read-granted
/// view the fixed actor may not read ([`Route::not_granted`]), before anything is read, its
/// path variables and query string included, as ESS's served surface checks the reader first.
///
/// An operation the route marks [`Unserved::Ingress`] answers `501` with `committed: false`,
/// naming the operation and what it uses, before the body is read or the host asked. A request
/// whose selected outcome the route refuses ([`Route::refused`]: one this ingress does not answer
/// as the binding lists it, or one the kernel does not take) answers `501` with
/// `committed: false`, naming the outcome and what it lacks, and nothing is minted or written;
/// every other outcome of the command answers as above.
pub fn answer(
    host: &mut Host,
    route: &Route,
    carried: &Map<String, Value>,
    query: &str,
    content_type: Option<&str>,
    body: &[u8],
) -> Answer {
    answered(host, route, carried, query, content_type, body).0
}

/// [`answer`], and every command a binding invoked on the way that did not execute: each was
/// dropped, and none runs again.
pub fn answered(
    host: &mut Host,
    route: &Route,
    carried: &Map<String, Value>,
    query: &str,
    content_type: Option<&str>,
    body: &[u8],
) -> (Answer, Vec<Failed>) {
    if let Some(unserved @ Unserved::Ingress(_)) = route.unserved() {
        return (
            Answer::unfinished(format!("{}: {unserved}", route.operation), false),
            Vec::new(),
        );
    }
    match &route.operation {
        Operation::Command(command) => {
            let input = match route.encoding() {
                Encoding::Json => input(body),
                Encoding::Form => form(route, content_type, body),
                Encoding::Query => route
                    .flat_input(query, "query")
                    .map_err(|detail| Answer::refusal(400, detail)),
            };
            match input {
                Ok(input) => run(host, route, command, carried, input),
                Err(refusal) => (refusal, Vec::new()),
            }
        }
        Operation::View(_) if route.not_granted() => (
            Answer::not_granted(route.caller().map_or("", |caller| caller.actor.as_str())),
            Vec::new(),
        ),
        Operation::View(view) => {
            let mut given = carried.clone();
            match route.parameters(query) {
                Ok(queried) => given.extend(queried),
                Err(why) => return (Answer::refusal(400, why), Vec::new()),
            }
            match host.list_with(view, &given) {
                Ok(rows) => (
                    Answer {
                        status: 200,
                        body: json!({"rows": rows}),
                    },
                    Vec::new(),
                ),
                Err(error @ HostError::Parameters { .. }) => {
                    (Answer::refusal(400, error.to_string()), Vec::new())
                }
                Err(error) => (Answer::refusal(501, error.to_string()), Vec::new()),
            }
        }
    }
}

fn run(
    host: &mut Host,
    route: &Route,
    command: &str,
    carried: &Map<String, Value>,
    mut input: Map<String, Value>,
) -> (Answer, Vec<Failed>) {
    if let Some(field) = input
        .keys()
        .find(|field| !route.input().iter().any(|declared| declared == *field))
    {
        return (
            Answer::refusal(400, format!("{command} declares no input field {field}")),
            Vec::new(),
        );
    }
    for (field, value) in carried {
        if input.contains_key(field) {
            return (
                Answer::refusal(
                    400,
                    format!("{field} is carried in the path, and the body carries it too"),
                ),
                Vec::new(),
            );
        }
        input.insert(field.clone(), value.clone());
    }
    let caller = route.caller();
    let answer = match host.submit_refusing(
        command,
        input,
        caller.map(|caller| caller.actor.as_str()),
        caller.map(|caller| &caller.attributes),
        &route.refusing(),
    ) {
        Ok(report) => {
            let answer = match report.ran.first() {
                Some(ran) => envelope(
                    route,
                    command,
                    &ran.executed.outcome,
                    &ran.executed.events,
                    None,
                    ran.executed.response.as_ref(),
                ),
                None => Answer::unfinished(format!("{command} ran and reported nothing"), true),
            };
            return (answer, report.failed);
        }
        Err(HostError::CommandRefused { outcome, error, .. }) => {
            envelope(route, command, &outcome, &[], Some(&error), None)
        }
        Err(HostError::Execute { error, .. }) => match *error {
            ExecuteError::InvalidInput(detail) => {
                Answer::refusal(400, format!("invalid input: {detail}"))
            }
            ExecuteError::NotGranted { actor } => Answer::not_granted(&actor),
            other => Answer::unfinished(other.to_string(), false),
        },
        Err(error @ HostError::NotAccepted(_)) => Answer::unfinished(error.to_string(), false),
        Err(error) => Answer::unfinished(error.to_string(), true),
    };
    (answer, Vec::new())
}

/// The command's input: the body as a JSON object, the empty object for an empty body.
fn input(body: &[u8]) -> Result<Map<String, Value>, Answer> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(Map::new());
    }
    match serde_json::from_slice::<Value>(body) {
        Ok(Value::Object(input)) => Ok(input),
        Ok(other) => Err(Answer::refusal(
            400,
            format!("the body is not a JSON object: {other}"),
        )),
        Err(error) => Err(Answer::refusal(
            400,
            format!("the body is not JSON: {error}"),
        )),
    }
}

/// The command's input from a `form` body: refused `400` unless `content_type` is
/// `application/x-www-form-urlencoded` (its parameters and case ignored) and the body UTF-8, as
/// ESS's served application refuses it (`ess-synth` `http::content_type_is`, `http::body_text`).
fn form(
    route: &Route,
    content_type: Option<&str>,
    body: &[u8],
) -> Result<Map<String, Value>, Answer> {
    let declared = content_type.is_some_and(|value| {
        value
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .eq_ignore_ascii_case(FORM)
    });
    if !declared {
        return Err(Answer::refusal(400, format!("the body is not `{FORM}`")));
    }
    let text = std::str::from_utf8(body)
        .map_err(|error| Answer::refusal(400, format!("the body is not UTF-8: {error}")))?;
    route
        .flat_input(text, "body")
        .map_err(|detail| Answer::refusal(400, detail))
}

/// The media type of a `form` request body.
const FORM: &str = "application/x-www-form-urlencoded";

/// The envelope ESS publishes for `outcome` of `command`, at the status `route` gives it.
fn envelope(
    route: &Route,
    command: &str,
    outcome: &str,
    emitted: &[Event],
    refusal: Option<&DeclaredError>,
    response: Option<&Map<String, Value>>,
) -> Answer {
    let Some(status) = route.status(outcome) else {
        return Answer::unfinished(
            format!("{command} selected {outcome}, which it does not declare"),
            refusal.is_none(),
        );
    };
    let mut body = Map::new();
    body.insert("outcome".to_owned(), Value::String(outcome.to_owned()));
    body.insert(
        "published".to_owned(),
        Value::Array(
            emitted
                .iter()
                .map(|event| json!({"event": event.name, "payload": event.payload}))
                .collect(),
        ),
    );
    if let Some(refusal) = refusal {
        body.insert("error".to_owned(), Value::String(refusal.name.clone()));
        if !refusal.fields.is_empty() && !refusal.unknown_instance {
            body.insert("payload".to_owned(), Value::Object(refusal.fields.clone()));
        }
    }
    if let Some(response) = response.filter(|_| route.responds(outcome)) {
        body.insert("response".to_owned(), Value::Object(response.clone()));
    }
    Answer {
        status,
        body: Value::Object(body),
    }
}

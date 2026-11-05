//! One request's answer, from the host that runs the command or reads the view.

use ess_http::Operation;
use serde_json::{Map, Value, json};
use spindle_host::{Cause, Host, HostError, Report};
use spindle_kernel::{DeclaredError, Event, ExecuteError};

use crate::surface::Route;

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

    /// The `501` the contract declares on every command: the realization is unfinished.
    /// `committed` says whether the command's effect was written before it stopped.
    pub fn unfinished(detail: impl Into<String>, committed: bool) -> Self {
        Self {
            status: 501,
            body: json!({"refused": detail.into(), "committed": committed}),
        }
    }
}

/// Answers one request for `route`, whose path carries `carried` and whose body is `body`, by
/// running it on `host`.
///
/// A command's body is its input as a JSON object, the `{command}.Input` ESS projects, less the
/// fields the path carries; an empty body is the empty input, and a field both carry is refused
/// `400`. It answers the status the route gives the outcome the model selected ([`Route::status`]:
/// the binding's, else ESS's derived one) with `{"outcome", "error"?, "events", "invocations"}`:
///
/// - `outcome`: the declared outcome's name;
/// - `error`: on a refusing outcome, `{"error": <qualified name>, "payload": {...}}`, the payload
///   present where the error declares fields;
/// - `events`: each event the command emitted, `{"event": <qualified name>, "payload": {...}}`, in
///   emit order;
/// - `invocations`: each command an event binding then ran, refused or failed to run, as
///   `{"binding", "component", "command"}` with its `outcome` and `events`, its `error` where
///   refused, or its `failure` in words where it did not run: those that ran first, then those
///   refused, then those that failed, each in the order run. Empty when no binding fired.
///
/// A view answers `200` with the body ESS projects for it, `{"rows": [...]}`, every row it holds.
///
/// A body that is not a JSON object, or input the command does not declare, is refused `400`. A
/// command the kernel does not run answers `501` with `committed: false`; one whose effect was
/// written and whose consequences could not be carried answers `501` with `committed: true`. A
/// view the host cannot answer is refused `501`, naming what it does not read.
pub fn answer(host: &mut Host, route: &Route, carried: &Map<String, Value>, body: &[u8]) -> Answer {
    match &route.operation {
        Operation::Command(command) => run(host, route, command, carried, body),
        Operation::View(view) => match host.list(view) {
            Ok(rows) => Answer {
                status: 200,
                body: json!({"rows": rows}),
            },
            Err(error) => Answer::refusal(501, error.to_string()),
        },
    }
}

fn run(
    host: &mut Host,
    route: &Route,
    command: &str,
    carried: &Map<String, Value>,
    body: &[u8],
) -> Answer {
    let mut input = match input(body) {
        Ok(input) => input,
        Err(refusal) => return refusal,
    };
    for (field, value) in carried {
        if input.contains_key(field) {
            return Answer::refusal(
                400,
                format!("{field} is carried in the path, and the body carries it too"),
            );
        }
        input.insert(field.clone(), value.clone());
    }
    match host.submit(command, input) {
        Ok(report) => match report.ran.first() {
            Some(ran) => envelope(
                route,
                command,
                &ran.executed.outcome,
                &ran.executed.events,
                None,
                invocations(&report),
            ),
            None => Answer::unfinished(format!("{command} ran and reported nothing"), true),
        },
        Err(HostError::CommandRefused { outcome, error, .. }) => {
            envelope(route, command, &outcome, &[], Some(&error), Vec::new())
        }
        Err(HostError::Execute { error, .. }) => match *error {
            ExecuteError::InvalidInput(detail) => {
                Answer::refusal(400, format!("invalid input: {detail}"))
            }
            other => Answer::unfinished(other.to_string(), false),
        },
        Err(error @ HostError::NotAccepted(_)) => Answer::unfinished(error.to_string(), false),
        Err(error) => Answer::unfinished(error.to_string(), true),
    }
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

fn events(events: &[Event]) -> Value {
    Value::Array(
        events
            .iter()
            .map(|event| json!({"event": event.name, "payload": event.payload}))
            .collect(),
    )
}

/// `{"error": <name>, "payload": {...}}`, the payload left out where the kernel determined none.
fn error(error: &DeclaredError) -> Value {
    let mut object = Map::new();
    object.insert("error".to_owned(), Value::String(error.name.clone()));
    if !error.fields.is_empty() {
        object.insert("payload".to_owned(), Value::Object(error.fields.clone()));
    }
    Value::Object(object)
}

/// The binding that caused a command, if one did.
fn binding(cause: &Cause) -> Value {
    match cause {
        Cause::Delivered { binding } | Cause::Tick { binding } => Value::String(binding.clone()),
        Cause::Submitted => Value::Null,
    }
}

/// Every command after the submitted one that `report` holds: ran, refused, then failed.
fn invocations(report: &Report) -> Vec<Value> {
    let ran = report.ran.iter().skip(1).map(|ran| {
        json!({
            "binding": binding(&ran.cause),
            "component": ran.component,
            "command": ran.command,
            "outcome": ran.executed.outcome,
            "events": events(&ran.executed.events),
        })
    });
    let refused = report.refused.iter().map(|refused| {
        json!({
            "binding": binding(&refused.cause),
            "component": refused.component,
            "command": refused.command,
            "outcome": refused.outcome,
            "events": [],
            "error": error(&refused.error),
        })
    });
    let failed = report.failed.iter().map(|failed| {
        json!({
            "binding": binding(&failed.cause),
            "component": failed.component,
            "command": failed.command,
            "failure": failed.error,
        })
    });
    ran.chain(refused).chain(failed).collect()
}

/// The answer to `outcome` of `command`, at the status `route` gives it.
fn envelope(
    route: &Route,
    command: &str,
    outcome: &str,
    emitted: &[Event],
    refusal: Option<&DeclaredError>,
    invoked: Vec<Value>,
) -> Answer {
    let Some(status) = route.status(outcome) else {
        return Answer::unfinished(
            format!("{command} selected {outcome}, which it does not declare"),
            refusal.is_none(),
        );
    };
    let mut body = Map::new();
    body.insert("outcome".to_owned(), Value::String(outcome.to_owned()));
    if let Some(refusal) = refusal {
        body.insert("error".to_owned(), error(refusal));
    }
    body.insert("events".to_owned(), events(emitted));
    body.insert("invocations".to_owned(), Value::Array(invoked));
    Answer {
        status,
        body: Value::Object(body),
    }
}

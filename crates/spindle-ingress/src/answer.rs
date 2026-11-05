//! One request's answer, from the host that runs the command or reads the view.

use ess_http::Operation;
use serde_json::{Map, Value, json};
use spindle_host::{Host, HostError};
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
/// A command's body is its input as a JSON object, the `{command}.Input` ESS publishes, less the
/// fields the path carries; an empty body is the empty input. A field the command does not
/// declare, or one both the path and the body carry, is refused `400`. It answers the status the
/// route gives the outcome the model selected ([`Route::status`]: the binding's, else ESS's
/// derived one) with the envelope ESS publishes for it: `outcome`, `published` (each event as
/// `{"event", "payload"}`, in emit order), and on a refusing outcome `error`, the error's
/// qualified name, with `payload` where the kernel determined its fields.
///
/// A view answers `200` with the body ESS publishes for it, `{"rows": [...]}`, every row it
/// holds.
///
/// A body that is not a JSON object, or input the kernel refuses, is refused `400`. A command the
/// kernel does not run answers `501` with `committed: false`; one whose effect was written and
/// whose consequences could not be carried answers `501` with `committed: true`. A view the host
/// cannot answer is refused `501`, naming what it does not read.
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
    if let Some(field) = input
        .keys()
        .find(|field| !route.input().iter().any(|declared| declared == *field))
    {
        return Answer::refusal(400, format!("{command} declares no input field {field}"));
    }
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
            ),
            None => Answer::unfinished(format!("{command} ran and reported nothing"), true),
        },
        Err(HostError::CommandRefused { outcome, error, .. }) => {
            envelope(route, command, &outcome, &[], Some(&error))
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

/// The envelope ESS publishes for `outcome` of `command`, at the status `route` gives it.
fn envelope(
    route: &Route,
    command: &str,
    outcome: &str,
    emitted: &[Event],
    refusal: Option<&DeclaredError>,
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
        if !refusal.fields.is_empty() {
            body.insert("payload".to_owned(), Value::Object(refusal.fields.clone()));
        }
    }
    Answer {
        status,
        body: Value::Object(body),
    }
}

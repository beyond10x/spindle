//! One request's answer, from the host that runs the command or reads the view.

use std::borrow::Cow;

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedCommand, ResolvedError, ResolvedField};
use ess_gen::http::{FORBIDDEN, NOT_GRANTED};
use ess_http::{Body, Encoding, Operation};
use serde_json::{Map, Value, json};
use spindle_host::{Failed, Host, HostError, Reader};
use spindle_kernel::{DeclaredError, Event, ExecuteError};

use crate::observe::Answered;
use crate::surface::{Caller, Route, Unserved, may_read, may_run};

/// A status and the JSON body written under it, where it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// The HTTP status.
    pub status: u16,
    /// The body, written as `application/json`; `None` for an answer with no body and no
    /// `Content-Type`, as a binding's `body: none` declares.
    pub body: Option<Value>,
}

impl Answer {
    /// A refusal this surface makes rather than the specification, at `status`:
    /// `{"refused": <detail>}`, as ESS's served applications write one.
    pub fn refusal(status: u16, detail: impl Into<String>) -> Self {
        Self {
            status,
            body: Some(json!({"refused": detail.into()})),
        }
    }

    /// ESS's standard refusal for an actor no grant admits, on a command or a read-granted view:
    /// `403` `{"refused": "not granted", "actor": <actor>}`.
    pub fn not_granted(actor: &str) -> Self {
        Self {
            status: FORBIDDEN.parse().unwrap_or(403),
            body: Some(json!({"refused": NOT_GRANTED, "actor": actor})),
        }
    }

    /// ESS's standard refusal for a request authenticated as no caller, on a route whose requests
    /// prove their caller ([`Route::proves`]) where this one proves none: `403`
    /// `{"refused": "not granted", "actor": null}`.
    pub fn not_proven() -> Self {
        Self {
            status: FORBIDDEN.parse().unwrap_or(403),
            body: Some(json!({"refused": NOT_GRANTED, "actor": null})),
        }
    }

    /// The `501` the contract declares on every command: the realization is unfinished.
    /// `committed` says whether the command's effect was written before it stopped.
    pub fn unfinished(detail: impl Into<String>, committed: bool) -> Self {
        Self {
            status: 501,
            body: Some(json!({"refused": detail.into(), "committed": committed})),
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
/// chose for its declared fields, each under its wire name and written as ESS publishes it at
/// every depth. An outcome the binding answers with another body
/// ([`Route::body`]) answers its status with that body alone, as ESS's served application writes
/// it: `response` the command's response, `response.<field>` the value of that one field (`null`
/// where the outcome answers no response), `{<member>: error}` on a refusing outcome an object
/// whose `<member>` is the error's wire name beside the error's fields under their wire names, and
/// `none` no body and no `Content-Type`. A refusal answered so writes nothing, as under the
/// envelope.
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
/// A command is sent as the route's fixed caller ([`Route::caller`]), else as no caller. The grant
/// is checked first, as ESS's served surface checks it, before the body, the query string or the
/// path variables are read ([`Route::not_granted`]): a command the fixed actor's `may:` does not
/// name, or a read-granted view it may not read, answers ESS's standard refusal, `403`
/// `{"refused": "not granted", "actor": <actor>}`; without a fixed caller, a command some actor's
/// `may:` names, or a read-granted view, answers the refusal for a request authenticated as no
/// caller, `403` `{"refused": "not granted", "actor": null}`. Nothing is read or written. A
/// command and a view no actor's grant names are answered as no caller.
///
/// On a route whose requests prove their caller ([`Route::proves`]) this function is handed no
/// proof, so it answers every request `403` `{"refused": "not granted", "actor": null}`
/// ([`Answer::not_proven`]); the listener hands the router the caller each request proves.
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
    let (answer, failed, _) = answering(
        host,
        route,
        None,
        Carried::Read(carried),
        query,
        content_type,
        body,
    );
    (answer, failed)
}

/// The input a request's path carries in its route's variables: read by the router
/// ([`crate::Resolved::Found`]), or, on a route whose requests prove their caller
/// ([`Route::proves`]), the request's path itself, read only once the caller and its grant are
/// checked.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Carried<'a> {
    /// What the router read from the path.
    Read(&'a Map<String, Value>),
    /// The request's path, not yet read.
    Unread(&'a str),
}

/// [`answered`], and who answered: the model where the command's envelope found a status for the
/// outcome it selected, with that outcome and the events it published, or where the view gave
/// its rows; the ingress for every other answer.
///
/// On a route whose requests prove their caller ([`Route::proves`]) the request is sent or read
/// as `proven`, the caller its header proves ([`Caller::proven`]), once the model binds it
/// ([`Caller::binds`]); with no caller proven, or one the model does not bind, it is refused
/// [`Answer::not_proven`] before anything else is read, run or written. A command the proven
/// actor's `may:` does not name, or a read-granted view it may not read, answers
/// [`Answer::not_granted`] naming it, before its body, query string or path is read: the router
/// leaves the path unread on such a route ([`Carried::Unread`]).
pub(crate) fn answering(
    host: &mut Host,
    route: &Route,
    proven: Option<&Caller>,
    carried: Carried<'_>,
    query: &str,
    content_type: Option<&str>,
    body: &[u8],
) -> (Answer, Vec<Failed>, Answered) {
    let (caller, carried) = match answered_as(host.ir(), route, proven, carried) {
        Ok(answered_as) => answered_as,
        Err(refusal) => return (refusal, Vec::new(), Answered::ingress()),
    };
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
                Ok(input) => run(host, route, caller, command, &carried, input),
                Err(refusal) => (refusal, Vec::new(), Answered::ingress()),
            }
        }
        Operation::View(view) => {
            let (answer, answered) = match viewed(route, &carried, query) {
                Ok(given) => listed(host.list_with(view, &given)),
                Err(refusal) => (refusal, Answered::ingress()),
            };
            (answer, Vec::new(), answered)
        }
    }
}

/// [`answering`] for a request for the view route `route`, read beside the host's thread through
/// its `reader` ([`Reader::list_with`]): the caller proven, the grants checked and the parameters
/// read exactly as [`answering`] checks and reads them, before anything is read. `None` for a
/// route that is no view.
#[cfg_attr(not(feature = "listener"), allow(dead_code))]
pub(crate) fn reading(
    reader: &Reader,
    route: &Route,
    proven: Option<&Caller>,
    carried: Carried<'_>,
    query: &str,
) -> Option<(Answer, Answered)> {
    let Operation::View(view) = &route.operation else {
        return None;
    };
    let carried = match answered_as(reader.ir(), route, proven, carried) {
        Ok((_, carried)) => carried,
        Err(refusal) => return Some((refusal, Answered::ingress())),
    };
    Some(match viewed(route, &carried, query) {
        Ok(given) => listed(reader.list_with(view, &given)),
        Err(refusal) => (refusal, Answered::ingress()),
    })
}

/// Who a request is answered as, and the input its path carries ([`answered_as`]).
type AnsweredAs<'a> = (Option<&'a Caller>, Cow<'a, Map<String, Value>>);

/// Who a request for `route` on a model `ir` is answered as, and the input its path carries: on
/// a route whose requests prove their caller, `proven` once the model binds it, and otherwise the
/// route's fixed caller, if any. The refusal, before anything is read, run or written, in this
/// order: [`Answer::not_proven`] where no caller is proven or the model does not bind it; the
/// `501` of an operation this ingress does not route ([`Unserved::Ingress`]); and the grant check
/// ([`granted`]). Only then is a path the router left unread read ([`Carried::Unread`]), `400`
/// for a variable that does not read.
fn answered_as<'a>(
    ir: &EssIr,
    route: &'a Route,
    proven: Option<&'a Caller>,
    carried: Carried<'a>,
) -> Result<AnsweredAs<'a>, Answer> {
    let caller = if route.proves() {
        Some(
            proven
                .filter(|caller| caller.binds(ir))
                .ok_or_else(Answer::not_proven)?,
        )
    } else {
        route.caller()
    };
    if let Some(unserved @ Unserved::Ingress(_)) = route.unserved() {
        return Err(Answer::unfinished(
            format!("{}: {unserved}", route.operation),
            false,
        ));
    }
    granted(ir, route, caller)?;
    let carried = match carried {
        Carried::Read(carried) => Cow::Borrowed(carried),
        Carried::Unread(path) => Cow::Owned(
            route
                .read_path(path)
                .map_err(|why| Answer::refusal(400, why))?,
        ),
    };
    Ok((caller, carried))
}

/// ESS's grant check, before anything of the request is read: a route the check refuses
/// whatever the request ([`Route::not_granted`]) answers [`Answer::not_granted`] naming the fixed
/// caller's actor, or [`Answer::not_proven`] without one; on a route whose requests prove their
/// caller, a command the proven actor's `may:` does not name, or a read-granted view it may not
/// read, answers [`Answer::not_granted`] naming it.
fn granted(ir: &EssIr, route: &Route, caller: Option<&Caller>) -> Result<(), Answer> {
    if route.not_granted() {
        return Err(caller.map_or_else(Answer::not_proven, |caller| {
            Answer::not_granted(&caller.actor)
        }));
    }
    if let Some(caller) = caller.filter(|_| route.proves()) {
        let may = match &route.operation {
            Operation::Command(command) => may_run(ir, &caller.actor, command),
            Operation::View(view) => may_read(ir, &caller.actor, view),
        };
        if !may {
            return Err(Answer::not_granted(&caller.actor));
        }
    }
    Ok(())
}

/// The parameter values a read at `route` is given: those the path carries and those the query
/// string holds ([`Route::parameters`]); `400` for a query string that does not read.
fn viewed(
    route: &Route,
    carried: &Map<String, Value>,
    query: &str,
) -> Result<Map<String, Value>, Answer> {
    let mut given = carried.clone();
    given.extend(
        route
            .parameters(query)
            .map_err(|why| Answer::refusal(400, why))?,
    );
    Ok(given)
}

/// The answer to a view read that listed `rows`: `200` `{"rows": [...]}`, answered by the model;
/// `400` for parameter values the view refuses, and `501` for a view the host cannot read.
fn listed(rows: Result<Vec<Map<String, Value>>, HostError>) -> (Answer, Answered) {
    match rows {
        Ok(rows) => (
            Answer {
                status: 200,
                body: Some(json!({"rows": rows})),
            },
            Answered::model(None, Vec::new()),
        ),
        Err(error @ HostError::Parameters { .. }) => {
            (Answer::refusal(400, error.to_string()), Answered::ingress())
        }
        Err(error) => (Answer::refusal(501, error.to_string()), Answered::ingress()),
    }
}

fn run(
    host: &mut Host,
    route: &Route,
    caller: Option<&Caller>,
    command: &str,
    carried: &Map<String, Value>,
    mut input: Map<String, Value>,
) -> (Answer, Vec<Failed>, Answered) {
    if let Some(field) = input
        .keys()
        .find(|field| !route.input().iter().any(|declared| declared == *field))
    {
        return (
            Answer::refusal(400, format!("{command} declares no input field {field}")),
            Vec::new(),
            Answered::ingress(),
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
                Answered::ingress(),
            );
        }
        input.insert(field.clone(), value.clone());
    }
    let answer = match host.submit_refusing(
        command,
        input,
        caller.map(|caller| caller.actor.as_str()),
        caller.map(|caller| &caller.attributes),
        &route.refusing(),
    ) {
        Ok(report) => {
            let (answer, answered) = match report.ran.first() {
                Some(ran) => (
                    envelope(
                        host.ir(),
                        route,
                        command,
                        &ran.executed.outcome,
                        &ran.executed.events,
                        None,
                        ran.executed.response.as_ref(),
                    ),
                    selected(route, &ran.executed.outcome, &ran.executed.events),
                ),
                None => (
                    Answer::unfinished(format!("{command} ran and reported nothing"), true),
                    Answered::ingress(),
                ),
            };
            return (answer, report.failed, answered);
        }
        Err(HostError::CommandRefused { outcome, error, .. }) => {
            let answered = selected(route, &outcome, &[]);
            let answer = envelope(host.ir(), route, command, &outcome, &[], Some(&error), None);
            return (answer, Vec::new(), answered);
        }
        Err(HostError::Execute { error, .. }) => match *error {
            ExecuteError::InvalidInput(detail) => {
                Answer::refusal(400, format!("invalid input: {detail}"))
            }
            ExecuteError::NotGranted { actor } => Answer::not_granted(&actor),
            other => Answer::unfinished(other.to_string(), false),
        },
        Err(error @ HostError::NotAccepted(_)) => Answer::unfinished(error.to_string(), false),
        // What ran before the stop is committed, and so is what it reported as not executed: the
        // unmet binding inputs among it are the request's ([`crate::observe::UnmetInput::of`]).
        Err(HostError::Runaway {
            deliveries,
            bindings,
            failed,
        }) => {
            let error = HostError::Runaway {
                deliveries,
                bindings,
                failed: Vec::new(),
            };
            return (
                Answer::unfinished(error.to_string(), true),
                failed,
                Answered::ingress(),
            );
        }
        Err(error) => Answer::unfinished(error.to_string(), true),
    };
    (answer, Vec::new(), Answered::ingress())
}

/// Who answered a command that selected `outcome`, publishing `events`: the model, where the
/// route gives the outcome a status and so answers its envelope; the ingress, where it does not.
fn selected(route: &Route, outcome: &str, events: &[Event]) -> Answered {
    if route.status(outcome).is_some() {
        Answered::model(
            Some(outcome.to_owned()),
            events.iter().map(|event| event.name.clone()).collect(),
        )
    } else {
        Answered::ingress()
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

/// The envelope ESS publishes for `outcome` of `command`, at the status `route` gives it, or the
/// body the binding answers in its place ([`bare`]). The command's response, in either, is
/// written as ESS publishes it ([`Written`]).
fn envelope(
    ir: &EssIr,
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
    let written = Written::of(ir, command);
    if let Some(body) = bare(route, outcome, response, refusal, &written) {
        return Answer { status, body };
    }
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
            body.insert(
                "payload".to_owned(),
                Value::Object(written.error_fields(outcome, refusal)),
            );
        }
    }
    if let Some(response) = response.filter(|_| route.responds(outcome)) {
        body.insert(
            "response".to_owned(),
            Value::Object(written.response(response)),
        );
    }
    Answer {
        status,
        body: Some(Value::Object(body)),
    }
}

/// The body the binding answers `outcome` with in place of the envelope, where it does
/// ([`Route::body`]): `Some(None)` for `none`; for `response`, or `response.<field>`, the response,
/// or the value of the one field the body names (by its declared name), bare and written as ESS
/// publishes it, `null` where `response` holds none; for `{<member>: error}` the error object
/// ([`Written::error_object`]). `None` for an outcome answered with the envelope.
fn bare(
    route: &Route,
    outcome: &str,
    response: Option<&Map<String, Value>>,
    refusal: Option<&DeclaredError>,
    written: &Written<'_>,
) -> Option<Option<Value>> {
    match route.body(outcome)? {
        Body::None => Some(None),
        Body::Response => Some(Some(response.map_or(Value::Null, |response| {
            Value::Object(written.response(response))
        }))),
        Body::ResponseField(field) => Some(Some(
            response
                .and_then(|response| response.get(field))
                .map_or(Value::Null, |held| written.field(field, held)),
        )),
        Body::Error { member } => Some(Some(written.error_object(outcome, member, refusal))),
        Body::Envelope => None,
    }
}

/// How the response of one command is written on the wire: each declared field under its wire
/// name, its value as ESS publishes it at every depth ([`crate::wire`]). The kernel keys its
/// response by declared name; the mapping is made here, where the answer is written, so the
/// envelope's `response` and a bare body never disagree.
struct Written<'ir> {
    ir: &'ir EssIr,
    command: Option<&'ir ResolvedCommand>,
    fields: &'ir [ResolvedField],
}

impl<'ir> Written<'ir> {
    /// The response fields of `command`, by qualified name; none for a name the IR does not
    /// declare, whose response is then written as it is.
    fn of(ir: &'ir EssIr, command: &str) -> Self {
        let command = ir
            .commands()
            .values()
            .find(|declared| declared.name.to_string() == command);
        let fields = command.map_or(&[][..], |declared| declared.response.as_slice());
        Self {
            ir,
            command,
            fields,
        }
    }

    /// The error that `outcome` of the command declares, where it names one.
    fn declared_error(&self, outcome: &str) -> Option<&'ir ResolvedError> {
        self.command
            .and_then(|command| {
                command
                    .outcomes
                    .iter()
                    .find(|declared| declared.name.to_string() == outcome)
            })
            .and_then(|outcome| outcome.error.as_ref())
            .map(|error| self.ir.error(error))
    }

    /// The fields `refusal` carries, each under its wire name and written as ESS publishes it:
    /// the envelope's `payload`, whose schema keys them as the error object's are keyed.
    fn error_fields(&self, outcome: &str, refusal: &DeclaredError) -> Map<String, Value> {
        let fields = self
            .declared_error(outcome)
            .map_or(&[][..], |error| error.fields.as_slice());
        crate::wire::fields(self.ir, fields, &refusal.fields)
    }

    /// The error object a binding's `{<member>: error}` answers `outcome` with, as ESS's served
    /// application writes it (`ess-synth` `http::error_object`, `ess-gen`
    /// `openapi::error_object`): `<member>` the wire name of the error the outcome reports
    /// (`naming.wire`, else its local name), beside the fields `refusal` carries, each under its
    /// wire name and written as ESS publishes it. No fields for the `wrong_state:` answer to an
    /// identity no record carries, which ESS answers without a payload.
    fn error_object(&self, outcome: &str, member: &str, refusal: Option<&DeclaredError>) -> Value {
        let declared = self.declared_error(outcome);
        let wire = declared.map_or_else(
            || {
                refusal.map_or_else(String::new, |refusal| {
                    refusal
                        .name
                        .rsplit('.')
                        .next()
                        .unwrap_or_default()
                        .to_owned()
                })
            },
            |error| error.naming.wire_or(&error.name).to_owned(),
        );
        let mut object = Map::new();
        object.insert(member.to_owned(), Value::String(wire));
        if let Some(refusal) = refusal.filter(|refusal| !refusal.unknown_instance) {
            let fields = declared.map_or(&[][..], |error| error.fields.as_slice());
            object.extend(crate::wire::fields(self.ir, fields, &refusal.fields));
        }
        Value::Object(object)
    }

    /// The whole response, keyed by wire name.
    fn response(&self, response: &Map<String, Value>) -> Map<String, Value> {
        crate::wire::fields(self.ir, self.fields, response)
    }

    /// The value `held` of the response field declared `field`, as written on the wire.
    fn field(&self, field: &str, held: &Value) -> Value {
        self.fields
            .iter()
            .find(|declared| declared.name == field)
            .map_or_else(
                || held.clone(),
                |declared| crate::wire::value(self.ir, &declared.type_ref, held),
            )
    }
}

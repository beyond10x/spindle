//! The reserved prefix `/__spindle/`, the request log answered under it
//! (`GET /__spindle/requests`, a `spindle.observe.RequestLog`) and the metrics
//! (`GET /__spindle/metrics`, [`metrics`]).
//!
//! A surface records requests only where it was made [`Surface::observing`](crate::Surface::observing):
//! it then holds a [`Recorder`], a ring of the last requests and the totals per [`Class`] since
//! the run started, the requests counted by method, route, operation, kind, class and status
//! class, their durations, and the resets of the run's stores. The listener records every request
//! it answers outside the prefix, timed on its own thread from the head read to the answer handed
//! back; a request under the prefix is neither recorded nor counted. The transport-free
//! [`route`](crate::route) reads no clock, so it records nothing; it answers the prefix as the
//! listener does.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use ess_gen::http::NOT_GRANTED;
use serde_json::{Map, Value, json};
use spindle_telemetry::DURATION_BOUNDS_US;

use crate::answer::Answer;
use crate::router::Response;

pub(crate) mod metrics;

/// The first path segment every route of spindle's own is under, and no specification route is.
pub const RESERVED_SEGMENT: &str = "__spindle";

/// How many requests a recorder keeps by default.
pub const REQUESTS_DEFAULT: usize = 256;

/// The most requests a recorder keeps.
pub const REQUESTS_MOST: usize = 10_000;

/// The format of the request log.
const FORMAT: &str = "spindle-requests/1";

/// The media type of the observe panel's page, `GET /__spindle/ui`.
pub const PANEL_MEDIA: &str = "text/html; charset=utf-8";

/// The `Content-Security-Policy` of the observe panel's page: its own inline script and style
/// run, it reads only its own origin, and it loads nothing else. It names no `frame-ancestors`,
/// and no `X-Frame-Options` is sent, so any page may frame it.
pub const PANEL_POLICY: &str = "default-src 'none'; script-src 'unsafe-inline'; \
     style-src 'unsafe-inline'; img-src data:; connect-src 'self'; base-uri 'none'; \
     form-action 'none'";

/// Where the observe panel's page is built, as a `503` without it names it.
const PANEL_BUILT: &str = "ui/dist-panel/panel.html";

/// A route under the reserved prefix: the epic lays out four; every other path under it is
/// [`Reserved::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reserved {
    /// `/__spindle/requests`: the request log.
    Requests,
    /// `/__spindle/metrics`: the metrics, as the Prometheus text format ([`metrics`]).
    Metrics,
    /// `/__spindle/ui`, the observe panel ([`crate::Surface::with_panel`]), and every path under
    /// it, which answers `404`.
    Ui,
    /// `/__spindle/topology`, answered `404` until its story lands.
    Topology,
    /// Any other path under the prefix: `404`.
    Other,
}

impl Reserved {
    /// The reserved route `path` (without its query string) is for, where its first segment,
    /// percent-decoded, is [`RESERVED_SEGMENT`]; `None` for every other path.
    pub fn of(path: &str) -> Option<Self> {
        let mut segments = path.strip_prefix('/')?.split('/');
        let first = segments.next()?;
        if crate::surface::decoded(first).as_deref() != Some(RESERVED_SEGMENT) {
            return None;
        }
        let rest = segments.collect::<Vec<_>>();
        Some(match rest.as_slice() {
            ["requests"] => Self::Requests,
            ["metrics"] => Self::Metrics,
            ["topology"] => Self::Topology,
            ["ui", ..] => Self::Ui,
            _ => Self::Other,
        })
    }
}

/// How a served run answered one request (`spindle.observe.RequestClass`); the class follows who
/// answered, not the status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// The model answered it: an outcome the command declares, whatever status the route gives
    /// it, or the view's rows.
    Served,
    /// The ingress refused it `4xx` before the model answered, other than [`Class::NotGranted`].
    Refused,
    /// `403` "not granted": no grant admits the actor.
    NotGranted,
    /// Spindle's own `501`: an operation or outcome this ingress, the kernel or the host does not
    /// serve.
    NotServed,
    /// A `5xx` spindle itself answered, other than that `501`.
    Failed,
    /// No route has the path, or not under that method.
    Unrouted,
}

impl Class {
    /// Every class, in the order the totals list them.
    pub const ALL: [Self; 6] = [
        Self::Served,
        Self::Refused,
        Self::NotGranted,
        Self::NotServed,
        Self::Failed,
        Self::Unrouted,
    ];

    /// The class of an answer with `status` and the `refused` text `refused`, answered `by`.
    pub fn of(status: u16, refused: Option<&str>, by: By) -> Self {
        match by {
            By::Model => Self::Served,
            By::Unrouted => Self::Unrouted,
            By::Ingress => match status {
                403 if refused == Some(NOT_GRANTED) => Self::NotGranted,
                501 => Self::NotServed,
                400..=499 => Self::Refused,
                500..=599 => Self::Failed,
                _ => Self::Served,
            },
        }
    }

    /// Its name on the wire, as `?class=` takes it: `not-granted`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Served => "served",
            Self::Refused => "refused",
            Self::NotGranted => "not-granted",
            Self::NotServed => "not-served",
            Self::Failed => "failed",
            Self::Unrouted => "unrouted",
        }
    }

    /// Its key among the totals: `not_granted`.
    fn total(self) -> &'static str {
        match self {
            Self::NotGranted => "not_granted",
            Self::NotServed => "not_served",
            other => other.name(),
        }
    }

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|class| *class == self)
            .unwrap_or_default()
    }

    fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|class| class.name() == name)
    }
}

impl fmt::Display for Class {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// Who answered a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By {
    /// The model, with an outcome the command declares or the view's rows.
    Model,
    /// The ingress, before or instead of the model: a refusal, a `501`, a console route.
    Ingress,
    /// The router, finding no route for the path or the method.
    Unrouted,
}

/// What answered one external cause of a served command (`spindle.observe.AnswerSource`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnswerSource {
    /// The decision table of the last reset's world.
    World,
    /// The decisions file, `--decisions`.
    File,
    /// The default branch `--answer-default` takes.
    Default,
    /// Nothing did, and the command answered `501`.
    Unanswered,
}

impl AnswerSource {
    /// Its name on the wire: `unanswered`.
    pub fn name(self) -> &'static str {
        match self {
            Self::World => "world",
            Self::File => "file",
            Self::Default => "default",
            Self::Unanswered => "unanswered",
        }
    }
}

impl fmt::Display for AnswerSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// One external cause a served command asked about, and what answered it
/// (`spindle.observe.ExternalAnswer`, with the command it was asked of).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalAnswer {
    /// The command that asked, by qualified name.
    pub command: String,
    /// The outcome whose cause was asked about.
    pub outcome: String,
    /// Whether the cause holds; `None` when nothing answered it.
    pub holds: Option<bool>,
    /// What answered it.
    pub source: AnswerSource,
}

impl ExternalAnswer {
    fn json(&self) -> Value {
        let mut answer = Map::new();
        answer.insert("outcome".to_owned(), json!(self.outcome));
        if let Some(holds) = self.holds {
            answer.insert("holds".to_owned(), json!(holds));
        }
        answer.insert("source".to_owned(), json!(self.source.name()));
        Value::Object(answer)
    }
}

/// What the model answered a request with, beside its status and body: who answered, the outcome
/// it selected, the events it published and the external causes its command asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Answered {
    pub(crate) by: By,
    pub(crate) outcome: Option<String>,
    pub(crate) events: Vec<String>,
    pub(crate) externals: Vec<ExternalAnswer>,
}

impl Answered {
    /// Answered by the ingress, with no outcome and no event.
    pub(crate) fn ingress() -> Self {
        Self {
            by: By::Ingress,
            outcome: None,
            events: Vec::new(),
            externals: Vec::new(),
        }
    }

    /// Answered by the model with `outcome`, publishing `events`.
    pub(crate) fn model(outcome: Option<String>, events: Vec<String>) -> Self {
        Self {
            by: By::Model,
            outcome,
            events,
            externals: Vec::new(),
        }
    }

    /// Who answered `answer`, read from it alone: the ingress where it carries a `refused` text,
    /// the model otherwise.
    pub(crate) fn of(answer: &Answer) -> Self {
        let refused = answer
            .body
            .as_ref()
            .is_some_and(|body| body.get("refused").is_some_and(Value::is_string));
        if refused {
            Self::ingress()
        } else {
            Self::model(None, Vec::new())
        }
    }
}

/// What a request was for, and how it was answered, as one record keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Note {
    pub(crate) route: Option<String>,
    pub(crate) operation: Option<String>,
    pub(crate) kind: Option<&'static str>,
    pub(crate) answered: Answered,
}

impl Note {
    /// A request no route was found for.
    pub(crate) fn unrouted(by: By) -> Self {
        Self {
            route: None,
            operation: None,
            kind: None,
            answered: Answered {
                by,
                ..Answered::ingress()
            },
        }
    }
}

/// One request as the log lists it (`spindle.observe.RequestRecord`).
#[derive(Debug, Clone)]
struct Record {
    sequence: u64,
    at: SystemTime,
    method: String,
    path: String,
    route: Option<String>,
    operation: Option<String>,
    kind: Option<&'static str>,
    status: u16,
    class: Class,
    outcome: Option<String>,
    reason: Option<String>,
    duration: Duration,
    events: Vec<String>,
    externals: Vec<ExternalAnswer>,
}

impl Record {
    fn json(&self) -> Value {
        let mut record = Map::new();
        record.insert("sequence".to_owned(), json!(self.sequence));
        record.insert(
            "at".to_owned(),
            json!(spindle_telemetry::timestamp(self.at)),
        );
        record.insert("method".to_owned(), json!(self.method));
        record.insert("path".to_owned(), json!(self.path));
        let optional = [
            ("route", self.route.as_deref()),
            ("operation", self.operation.as_deref()),
            ("kind", self.kind),
        ];
        for (key, value) in optional {
            if let Some(value) = value {
                record.insert(key.to_owned(), json!(value));
            }
        }
        record.insert("status".to_owned(), json!(self.status));
        record.insert("class".to_owned(), json!(self.class.name()));
        let optional = [
            ("outcome", self.outcome.as_deref()),
            ("reason", self.reason.as_deref()),
        ];
        for (key, value) in optional {
            if let Some(value) = value {
                record.insert(key.to_owned(), json!(value));
            }
        }
        record.insert(
            "duration_us".to_owned(),
            json!(u64::try_from(self.duration.as_micros()).unwrap_or(u64::MAX)),
        );
        record.insert("events".to_owned(), json!(self.events));
        record.insert(
            "externals".to_owned(),
            Value::Array(self.externals.iter().map(ExternalAnswer::json).collect()),
        );
        Value::Object(record)
    }
}

#[derive(Debug)]
struct Ring {
    capacity: usize,
    recorded: u64,
    totals: [u64; 6],
    records: VecDeque<Record>,
    /// Every request recorded, by its labels ([`Series`]).
    requests: BTreeMap<Series, u64>,
    /// The durations of every request recorded, one count per bucket of [`DURATION_BOUNDS_US`]
    /// and one above the last.
    durations: Vec<u64>,
    /// The sum of those durations, in microseconds.
    duration_sum_us: u64,
    /// Resets of the run's stores through `POST /api/reset`.
    resets: u64,
    /// Every external cause answered, by command, outcome and source.
    externals: BTreeMap<[String; 3], u64>,
    /// The external causes answered since the host's thread last took them
    /// ([`Recorder::take_answers`]), in the order asked.
    answers: Vec<ExternalAnswer>,
}

/// The labels a request is counted by: method, route template, operation, kind, class and status
/// class (`2xx`), the route, operation and kind empty for an unrouted request.
pub(crate) type Series = [String; 6];

/// The methods RFC 9110 registers. Any other method is counted as `other`, so a client cannot
/// add a series by inventing one; every route has one of them.
const METHODS: [&str; 9] = [
    "GET", "HEAD", "POST", "PUT", "DELETE", "CONNECT", "OPTIONS", "TRACE", "PATCH",
];

/// What a recorder counted since the run started, read for the metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Traffic {
    /// Every request, by its labels, in label order.
    pub(crate) requests: Vec<(Series, u64)>,
    /// One count per duration bucket of [`DURATION_BOUNDS_US`] and one above the last, each
    /// counting only its own bucket.
    pub(crate) durations: Vec<u64>,
    /// The sum of every duration, in microseconds.
    pub(crate) duration_sum_us: u64,
    /// Resets of the run's stores.
    pub(crate) resets: u64,
    /// Every external cause answered, by command, outcome and source, in label order.
    pub(crate) externals: Vec<([String; 3], u64)>,
}

/// The requests a served run answered: the last of them, at most its capacity, and the totals
/// per [`Class`] since it started. Clones share one ring.
#[derive(Clone)]
pub struct Recorder(Arc<Mutex<Ring>>);

impl Recorder {
    /// A recorder keeping the last `capacity` requests, `capacity` held to 1 through
    /// [`REQUESTS_MOST`].
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.clamp(1, REQUESTS_MOST);
        Self(Arc::new(Mutex::new(Ring {
            capacity,
            recorded: 0,
            totals: [0; 6],
            records: VecDeque::with_capacity(capacity),
            requests: BTreeMap::new(),
            durations: vec![0; DURATION_BOUNDS_US.len() + 1],
            duration_sum_us: 0,
            resets: 0,
            externals: BTreeMap::new(),
            answers: Vec::new(),
        })))
    }

    /// How many requests it keeps at most.
    pub fn capacity(&self) -> usize {
        self.ring(|ring| ring.capacity)
    }

    /// Records one request: `method` and `path` (without the query string), its head read `at`,
    /// answered with `response` after `duration`, for what `note` says. Its class is read from
    /// the answer ([`Class::of`]); its reason is the answer's `refused` text, for every class but
    /// [`Class::Served`]. The oldest record is dropped beyond the capacity; the request stays
    /// counted by its labels and its duration.
    pub(crate) fn record(
        &self,
        at: SystemTime,
        duration: Duration,
        method: &str,
        path: &str,
        response: &Response,
        note: Note,
    ) {
        let refused = refused(&response.body);
        let class = Class::of(response.status, refused.as_deref(), note.answered.by);
        let micros = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
        let series: Series = [
            if METHODS.contains(&method) {
                method
            } else {
                "other"
            }
            .to_owned(),
            note.route.clone().unwrap_or_default(),
            note.operation.clone().unwrap_or_default(),
            note.kind.unwrap_or_default().to_owned(),
            class.name().to_owned(),
            format!("{}xx", response.status / 100),
        ];
        self.ring(|ring| {
            *ring.requests.entry(series).or_default() += 1;
            let bucket = DURATION_BOUNDS_US
                .iter()
                .position(|bound| micros <= *bound)
                .unwrap_or(DURATION_BOUNDS_US.len());
            ring.durations[bucket] += 1;
            ring.duration_sum_us = ring.duration_sum_us.saturating_add(micros);
            let record = Record {
                sequence: ring.recorded,
                at,
                method: method.to_owned(),
                path: path.to_owned(),
                route: note.route,
                operation: note.operation,
                kind: note.kind,
                status: response.status,
                class,
                outcome: note.answered.outcome,
                reason: refused.filter(|_| class != Class::Served),
                duration,
                events: note.answered.events,
                externals: note.answered.externals,
            };
            ring.recorded += 1;
            ring.totals[class.index()] += 1;
            if ring.records.len() == ring.capacity {
                ring.records.pop_back();
            }
            ring.records.push_front(record);
        });
    }

    /// The request log (`spindle.observe.RequestLog`): the totals since the run started and the
    /// records the ring holds, newest first, only those of `classes` where it names any.
    pub fn log(&self, classes: &[Class]) -> Value {
        self.ring(|ring| {
            let totals = Class::ALL
                .into_iter()
                .map(|class| (class.total().to_owned(), json!(ring.totals[class.index()])))
                .collect::<Map<_, _>>();
            let requests = ring
                .records
                .iter()
                .filter(|record| classes.is_empty() || classes.contains(&record.class))
                .map(Record::json)
                .collect::<Vec<_>>();
            json!({
                "format": FORMAT,
                "capacity": ring.capacity,
                "recorded": ring.recorded,
                "totals": totals,
                "requests": requests,
            })
        })
    }

    /// Counts one reset of the run's stores; it clears no count and no record.
    pub(crate) fn count_reset(&self) {
        self.ring(|ring| ring.resets += 1);
    }

    /// Notes one external cause a command asked about on the host's thread, and what answered
    /// it: counted at once by command, outcome and source, and listed in the record of the
    /// request the host is answering, where its command is the one that asked.
    pub fn answered(&self, answer: ExternalAnswer) {
        self.ring(|ring| {
            let series = [
                answer.command.clone(),
                answer.outcome.clone(),
                answer.source.name().to_owned(),
            ];
            *ring.externals.entry(series).or_default() += 1;
            ring.answers.push(answer);
        });
    }

    /// The external causes answered since the last call, in the order asked.
    pub(crate) fn take_answers(&self) -> Vec<ExternalAnswer> {
        self.ring(|ring| std::mem::take(&mut ring.answers))
    }

    /// What it counted since the run started.
    pub(crate) fn traffic(&self) -> Traffic {
        self.ring(|ring| Traffic {
            requests: ring
                .requests
                .iter()
                .map(|(series, count)| (series.clone(), *count))
                .collect(),
            durations: ring.durations.clone(),
            duration_sum_us: ring.duration_sum_us,
            resets: ring.resets,
            externals: ring
                .externals
                .iter()
                .map(|(series, count)| (series.clone(), *count))
                .collect(),
        })
    }

    fn ring<T>(&self, read: impl FnOnce(&mut Ring) -> T) -> T {
        read(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl fmt::Debug for Recorder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Recorder")
            .field("capacity", &self.capacity())
            .finish_non_exhaustive()
    }
}

/// Two recorders are equal when they share one ring.
impl PartialEq for Recorder {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Recorder {}

/// The `refused` text of a JSON answer body, where it has one.
fn refused(body: &str) -> Option<String> {
    let body: Value = serde_json::from_str(body).ok()?;
    body.get("refused")?.as_str().map(str::to_owned)
}

/// Answers a request for the reserved route `reserved` with `method`, `path` and `query` (after
/// the `?`, still percent-encoded): without a recorder every path under the prefix answers `404`;
/// with one, `GET /__spindle/requests` answers the request log, each `class=<class>` pair of the
/// query keeping only that class, and any other method `405`; `GET /__spindle/ui` answers the
/// panel's page `panel`, `503` naming it without one; `GET /__spindle/metrics` is answered on the
/// host's thread, since it reads the stores ([`metrics::exposition`]), so it answers `None` here,
/// and any other method `405`. Every other reserved path answers `404`. A query pair other than a
/// known `class` is refused `400`.
pub(crate) fn reserved(
    recorder: Option<&Recorder>,
    panel: Option<&'static str>,
    reserved: Reserved,
    method: &str,
    path: &str,
    query: &str,
) -> Option<Response> {
    let not_found = || Answer::refusal(404, format!("no route serves {path}")).into();
    let Some(recorder) = recorder else {
        return Some(not_found());
    };
    let only_get = || {
        let mut response: Response = Answer::refusal(405, "this path answers GET").into();
        response.allow = Some("GET".to_owned());
        response
    };
    Some(match reserved {
        Reserved::Requests if method == "GET" => match classes(query) {
            Ok(classes) => Response {
                status: 200,
                allow: None,
                panel: false,
                metrics: false,
                body: recorder.log(&classes).to_string(),
            },
            Err(why) => Answer::refusal(400, why).into(),
        },
        Reserved::Requests | Reserved::Metrics if method != "GET" => only_get(),
        Reserved::Metrics => return None,
        Reserved::Ui => self::panel(panel, method, path).unwrap_or_else(not_found),
        Reserved::Requests | Reserved::Topology | Reserved::Other => not_found(),
    })
}

/// The answer to `GET /__spindle/metrics` on `host`, the requests `recorder` counted: `200` with
/// the text exposition ([`metrics::METRICS_MEDIA`]), `500` naming why a store could not be read.
pub(crate) fn metrics(recorder: &Recorder, host: &mut spindle_host::Host) -> Response {
    match metrics::exposition(recorder, host) {
        Ok(body) => Response {
            status: 200,
            allow: None,
            panel: false,
            metrics: true,
            body,
        },
        Err(error) => Answer::refusal(500, error.to_string()).into(),
    }
}

/// The answer to `method` at `path`, a path under `/__spindle/ui`: the page `panel` to `GET` at
/// `/__spindle/ui` itself, `503` naming the page without one, and `405` to any other method;
/// `None` for a path below it, which the page never reads.
fn panel(panel: Option<&'static str>, method: &str, path: &str) -> Option<Response> {
    if path.split('/').count() != 3 {
        return None;
    }
    Some(match panel {
        _ if method != "GET" => {
            let mut response: Response = Answer::refusal(405, "this path answers GET").into();
            response.allow = Some("GET".to_owned());
            response
        }
        Some(page) => Response {
            status: 200,
            allow: None,
            panel: true,
            metrics: false,
            body: page.to_owned(),
        },
        None => Answer::refusal(
            503,
            format!("this build carries no observe panel: {PANEL_BUILT} was not built before it"),
        )
        .into(),
    })
}

/// The classes the `class=<class>` pairs of `query` name; the refusal of any other pair.
fn classes(query: &str) -> Result<Vec<Class>, String> {
    let mut classes = Vec::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = crate::surface::form_decoded(key).unwrap_or_else(|| key.to_owned());
        let value = crate::surface::form_decoded(value).unwrap_or_else(|| value.to_owned());
        if key != "class" {
            return Err(format!(
                "the request log reads only `class` from the query string, not `{key}`"
            ));
        }
        let Some(class) = Class::named(&value) else {
            let names = Class::ALL.map(Class::name).join(", ");
            return Err(format!("class `{value}` is not one of {names}"));
        };
        classes.push(class);
    }
    Ok(classes)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{Answered, By, Class, Note, Recorder, Reserved};
    use crate::answer::Answer;
    use crate::router::Response;

    /// The class of `answer`, answered `by`, as a record reads it.
    fn class(answer: Answer, by: By) -> Class {
        let response = Response::from(answer);
        Class::of(
            response.status,
            super::refused(&response.body).as_deref(),
            by,
        )
    }

    #[test]
    fn a_request_is_classed_by_its_answer() {
        let declared = |status| Answer {
            status,
            body: Some(serde_json::json!({"outcome": "done", "published": []})),
        };
        assert_eq!(class(declared(201), By::Model), Class::Served);
        assert_eq!(class(declared(410), By::Model), Class::Served);
        assert_eq!(
            class(Answer::refusal(400, "invalid input"), By::Ingress),
            Class::Refused
        );
        assert_eq!(
            class(Answer::not_granted("quire.notebook.Reader"), By::Ingress),
            Class::NotGranted
        );
        assert_eq!(
            class(
                Answer::refusal(403, "the Host header names neither"),
                By::Ingress
            ),
            Class::Refused,
            "a 403 other than \"not granted\" is a refusal"
        );
        assert_eq!(
            class(Answer::unfinished("not run", false), By::Ingress),
            Class::NotServed
        );
        assert_eq!(
            class(Answer::refusal(500, "broken"), By::Ingress),
            Class::Failed
        );
        assert_eq!(
            class(Answer::refusal(404, "no route serves /x"), By::Unrouted),
            Class::Unrouted
        );
        assert_eq!(
            class(Answer::refusal(405, "this path answers GET"), By::Unrouted),
            Class::Unrouted
        );
        assert_eq!(
            class(declared(501), By::Model),
            Class::Served,
            "a declared outcome is served whatever its status"
        );
    }

    #[test]
    fn the_reserved_prefix_is_its_first_segment_decoded() {
        assert_eq!(
            Reserved::of("/__spindle/requests"),
            Some(Reserved::Requests)
        );
        assert_eq!(
            Reserved::of("/%5F%5Fspindle/metrics"),
            Some(Reserved::Metrics)
        );
        assert_eq!(Reserved::of("/__spindle/ui/app.js"), Some(Reserved::Ui));
        assert_eq!(Reserved::of("/__spindle"), Some(Reserved::Other));
        assert_eq!(Reserved::of("/__spindle/requests/x"), Some(Reserved::Other));
        assert_eq!(Reserved::of("/x/__spindle"), None);
        assert_eq!(Reserved::of("/__spindles/requests"), None);
    }

    #[test]
    fn the_log_keeps_the_newest_and_counts_every_one() {
        let recorder = Recorder::new(2);
        for status in [404, 405, 404] {
            recorder.record(
                UNIX_EPOCH,
                Duration::from_micros(7),
                "GET",
                "/x",
                &Answer::refusal(status, "no").into(),
                Note::unrouted(By::Unrouted),
            );
        }
        let log = recorder.log(&[]);
        assert_eq!(log["recorded"], 3, "{log}");
        assert_eq!(log["requests"][0]["sequence"], 2, "{log}");
        assert_eq!(log["requests"][1]["sequence"], 1, "{log}");
        assert_eq!(log["requests"][0]["at"], "1970-01-01T00:00:00.000Z");
        assert_eq!(log["totals"]["unrouted"], 3, "{log}");
        assert_eq!(
            recorder.log(&[Class::Served])["requests"],
            serde_json::json!([])
        );
        let _ = Answered::ingress();
    }

    #[test]
    fn a_recorder_holds_its_capacity_within_bounds() {
        assert_eq!(Recorder::new(0).capacity(), 1);
        assert_eq!(Recorder::new(20_000).capacity(), super::REQUESTS_MOST);
    }
}

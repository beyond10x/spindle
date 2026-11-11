//! The reserved prefix `/__spindle/`, the request log answered under it
//! (`GET /__spindle/requests`, a `spindle.observe.RequestLog`), the metrics
//! (`GET /__spindle/metrics`, [`metrics`]) and the topology (`GET /__spindle/topology`,
//! [`topology`]).
//!
//! A surface records requests only where it was made [`Surface::observing`](crate::Surface::observing):
//! it then holds a [`Recorder`], a ring of the last requests and the totals per [`Class`] since
//! the run started, the requests counted by method, route, operation, kind, class and status
//! class, their durations, and the resets of the run's stores. The listener records every request
//! it answers outside the prefix, timed on its own thread from the head read to the answer handed
//! back; a request under the prefix is neither recorded nor counted. The transport-free
//! [`route`](crate::route) reads no request clock, so it records nothing; it answers the prefix
//! as the listener does. A metrics scrape is timed where the crate is built with its listener
//! ([`clock`]), never in the browser's build.
//!
//! A record keeps a bounded reason ([`reason`]): the answer's `refused` text with every value in
//! it written `…`, at most [`REASON_MOST`] bytes, so a refused value is never recorded. Each
//! unmet binding input it lists keeps its reason bounded the same way ([`UnmetInput::of`]).

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use ess_gen::http::NOT_GRANTED;
use serde_json::{Map, Value, json};
use spindle_host::{Cause, Failed};
use spindle_telemetry::DURATION_BOUNDS_US;

use crate::answer::{Answer, NO_INPUT_FIELD};
use crate::router::Response;
use crate::surface::NO_FIELD_NAMED;

pub(crate) mod metrics;
pub(crate) mod topology;

/// The first path segment every route of spindle's own is under, and no specification route is.
pub const RESERVED_SEGMENT: &str = "__spindle";

/// How many requests a recorder keeps by default.
pub const REQUESTS_DEFAULT: usize = 256;

/// The most requests a recorder keeps.
pub const REQUESTS_MOST: usize = 10_000;

/// The format of the request log.
const FORMAT: &str = "spindle-requests/1";

/// The most bytes of a record's reason (`spindle.observe.RequestRecord.reason`).
pub(crate) const REASON_MOST: usize = 512;

/// The most bytes of a record's path (`spindle.observe.RequestRecord.path`).
pub(crate) const PATH_MOST: usize = 2_048;

/// The longest name in backticks a reason keeps; a longer one is written `…`.
const NAME_MOST: usize = 64;

/// What a reason writes in place of a value.
const ELIDED: &str = "…";

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
    /// `/__spindle/topology`: the topology, a `spindle.observe.Topology` ([`topology`]).
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
    /// The HTTP call the environment binds the cause to (`[http.<name>]`).
    Call,
    /// Nothing did, or the cause's HTTP call failed, and the command answered `501`.
    Unanswered,
}

impl AnswerSource {
    /// Its name on the wire: `unanswered`.
    pub fn name(self) -> &'static str {
        match self {
            Self::World => "world",
            Self::File => "file",
            Self::Default => "default",
            Self::Call => "call",
            Self::Unanswered => "unanswered",
        }
    }
}

impl fmt::Display for AnswerSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// One external cause a command asked about, and what answered it
/// (`spindle.observe.ExternalAnswer`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalAnswer {
    /// The command that asked, by qualified name: the `command` label it is counted by.
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
        answer.insert("command".to_owned(), json!(self.command));
        answer.insert("outcome".to_owned(), json!(self.outcome));
        if let Some(holds) = self.holds {
            answer.insert("holds".to_owned(), json!(holds));
        }
        answer.insert("source".to_owned(), json!(self.source.name()));
        Value::Object(answer)
    }
}

/// One event binding whose input the event did not fill, or whose payload condition decided
/// Unknown, so its command was not invoked
/// (`spindle.observe.UnmetInput`, `docs/design/spindle.md` § 11, unmet binding input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnmetInput {
    binding: String,
    component: String,
    command: String,
    reason: String,
}

impl UnmetInput {
    /// The unmet input `failed` reports, if it is one: a binding's command that was never sent,
    /// zero attempts ([`spindle_host::Failed::attempts`]). Its reason is the host's with every
    /// value in it elided and cut to [`REASON_MOST`] bytes ([`reason`]), as a record's is: the
    /// value the event carried is never recorded.
    pub(crate) fn of(failed: &Failed) -> Option<Self> {
        match &failed.cause {
            Cause::Delivered { binding } if failed.attempts == 0 => Some(Self {
                binding: binding.clone(),
                component: failed.component.clone(),
                command: failed.command.clone(),
                reason: reason(&failed.error),
            }),
            _ => None,
        }
    }

    fn json(&self) -> Value {
        json!({
            "binding": self.binding,
            "component": self.component,
            "command": self.command,
            "reason": self.reason,
        })
    }
}

/// What the model answered a request with, beside its status and body: who answered, the outcome
/// it selected, the events it published, every external cause asked while it was answered and
/// every event binding whose input was unmet on the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Answered {
    pub(crate) by: By,
    pub(crate) outcome: Option<String>,
    pub(crate) events: Vec<String>,
    pub(crate) externals: Vec<ExternalAnswer>,
    pub(crate) unmet: Vec<UnmetInput>,
}

impl Answered {
    /// Answered by the ingress, with no outcome and no event.
    pub(crate) fn ingress() -> Self {
        Self {
            by: By::Ingress,
            outcome: None,
            events: Vec::new(),
            externals: Vec::new(),
            unmet: Vec::new(),
        }
    }

    /// Answered by the model with `outcome`, publishing `events`.
    pub(crate) fn model(outcome: Option<String>, events: Vec<String>) -> Self {
        Self {
            by: By::Model,
            outcome,
            events,
            externals: Vec::new(),
            unmet: Vec::new(),
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
    unmet: Vec<UnmetInput>,
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
        record.insert(
            "unmet".to_owned(),
            Value::Array(self.unmet.iter().map(UnmetInput::json).collect()),
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
    /// The durations of every request recorded, by method and route template ([`Timed`]).
    durations: BTreeMap<[String; 2], Timed>,
    /// How long the last metrics scrape took, in microseconds; `None` before the first.
    scraped_us: Option<u64>,
    /// Every entity a scrape found holding a row: its row count is written from then on.
    rows_seen: BTreeSet<String>,
    /// Resets of the run's stores through `POST /api/reset`.
    resets: u64,
    /// Every external cause answered, by command, outcome and source.
    externals: BTreeMap<[String; 3], u64>,
    /// Whether the host's thread is answering a request ([`Recorder::serving`]).
    serving: bool,
    /// The external causes asked since the request being answered began, in the order asked;
    /// empty while none is.
    answers: Vec<ExternalAnswer>,
    /// The routes of the surface it records for, each a `routes` edge of the topology.
    routes: Vec<topology::Routed>,
}

/// The labels a request is counted by: method, route template, operation, kind, class and status
/// class (`2xx`), the route, operation and kind empty for an unrouted request.
pub(crate) type Series = [String; 6];

/// The durations of the requests of one method and route template: one count per bucket of
/// [`DURATION_BOUNDS_US`] and one above the last, each counting only its own bucket, and their
/// sum in microseconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Timed {
    pub(crate) counts: Vec<u64>,
    pub(crate) sum_us: u64,
}

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
    /// The durations of every request, by method and route template, in label order.
    pub(crate) durations: Vec<([String; 2], Timed)>,
    /// How long the last metrics scrape took, in microseconds; `None` before the first.
    pub(crate) scraped_us: Option<u64>,
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
            durations: BTreeMap::new(),
            scraped_us: None,
            rows_seen: BTreeSet::new(),
            resets: 0,
            externals: BTreeMap::new(),
            serving: false,
            answers: Vec::new(),
            routes: Vec::new(),
        })))
    }

    /// This recorder recording for the surface whose routes are `routes`.
    #[must_use]
    pub(crate) fn routing(self, routes: Vec<topology::Routed>) -> Self {
        self.ring(|ring| ring.routes = routes);
        self
    }

    /// The routes of the surface it records for ([`Recorder::routing`]).
    pub(crate) fn routes(&self) -> Vec<topology::Routed> {
        self.ring(|ring| ring.routes.clone())
    }

    /// The sequences of the newest records the ring holds for `method` at the route template
    /// `route`, newest first, at most `most`.
    pub(crate) fn latest(&self, method: &str, route: &str, most: usize) -> Vec<u64> {
        self.ring(|ring| {
            ring.records
                .iter()
                .filter(|record| record.method == method && record.route.as_deref() == Some(route))
                .take(most)
                .map(|record| record.sequence)
                .collect()
        })
    }

    /// How many requests it keeps at most.
    pub fn capacity(&self) -> usize {
        self.ring(|ring| ring.capacity)
    }

    /// Records one request: `method` and `path` (without the query string, cut to
    /// [`PATH_MOST`] bytes with `…` where longer: [`recorded_path`]), its head read `at`,
    /// answered with `response` after `duration`, for what `note` says. Its class is read from
    /// the answer ([`Class::of`]); its reason is the answer's `refused` text with every value in
    /// it elided and cut to [`REASON_MOST`] bytes ([`reason`]), for every class but
    /// [`Class::Served`]. The oldest record is dropped beyond the capacity; the request stays
    /// counted by its labels and its duration, by method and route template.
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
        let counted = if METHODS.contains(&method) {
            method
        } else {
            "other"
        };
        let timed = [counted.to_owned(), note.route.clone().unwrap_or_default()];
        let series: Series = [
            counted.to_owned(),
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
            let durations = ring.durations.entry(timed).or_insert_with(|| Timed {
                counts: vec![0; DURATION_BOUNDS_US.len() + 1],
                sum_us: 0,
            });
            durations.counts[bucket] += 1;
            durations.sum_us = durations.sum_us.saturating_add(micros);
            let record = Record {
                sequence: ring.recorded,
                at,
                method: method.to_owned(),
                path: recorded_path(path),
                route: note.route,
                operation: note.operation,
                kind: note.kind,
                status: response.status,
                class,
                outcome: note.answered.outcome,
                reason: refused
                    .filter(|_| class != Class::Served)
                    .map(|refused| reason(&refused)),
                duration,
                events: note.answered.events,
                externals: note.answered.externals,
                unmet: note.answered.unmet,
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
    /// it: counted at once by command, outcome and source, and, while the host answers a request
    /// ([`Recorder::serving`]), listed in that request's record, whichever command asked it: the
    /// routed one or one an event binding invoked on the way. One asked while no request is
    /// answered, by a command a timer fired, is counted and kept nowhere.
    pub fn answered(&self, answer: ExternalAnswer) {
        self.ring(|ring| {
            let series = [
                answer.command.clone(),
                answer.outcome.clone(),
                answer.source.name().to_owned(),
            ];
            *ring.externals.entry(series).or_default() += 1;
            if ring.serving {
                ring.answers.push(answer);
            }
        });
    }

    /// The host's thread begins answering a request: every external cause noted from now until
    /// [`Recorder::served`] is that request's.
    pub(crate) fn serving(&self) {
        self.ring(|ring| {
            ring.serving = true;
            ring.answers.clear();
        });
    }

    /// The host's thread has answered the request [`Recorder::serving`] began: the external
    /// causes asked since, in the order asked. Until the next request begins, none is kept.
    pub(crate) fn served(&self) -> Vec<ExternalAnswer> {
        self.ring(|ring| {
            ring.serving = false;
            std::mem::take(&mut ring.answers)
        })
    }

    /// What it counted since the run started.
    pub(crate) fn traffic(&self) -> Traffic {
        self.ring(|ring| Traffic {
            requests: ring
                .requests
                .iter()
                .map(|(series, count)| (series.clone(), *count))
                .collect(),
            durations: ring
                .durations
                .iter()
                .map(|(series, timed)| (series.clone(), timed.clone()))
                .collect(),
            scraped_us: ring.scraped_us,
            resets: ring.resets,
            externals: ring
                .externals
                .iter()
                .map(|(series, count)| (series.clone(), *count))
                .collect(),
        })
    }

    /// Notes that a metrics scrape took `took`: the next scrape writes it.
    pub(crate) fn scraped(&self, took: Duration) {
        let micros = u64::try_from(took.as_micros()).unwrap_or(u64::MAX);
        self.ring(|ring| ring.scraped_us = Some(micros));
    }

    /// The rows of `rows` a scrape writes: each entity holding a row now, and each a scrape
    /// found holding one before, whatever it holds now. An entity that never held a row is not
    /// written.
    pub(crate) fn observed_rows(&self, rows: Vec<(String, usize)>) -> Vec<(String, usize)> {
        self.ring(|ring| {
            rows.into_iter()
                .filter(|(entity, held)| {
                    if ring.rows_seen.contains(entity) {
                        return true;
                    }
                    if *held > 0 {
                        ring.rows_seen.insert(entity.clone());
                    }
                    *held > 0
                })
                .collect()
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

/// The reason a record keeps of the `refused` text: its words and the names it gives, with every
/// value in it written `…`, at most [`REASON_MOST`] bytes, cut with `…` where longer. A value is
/// a quoted string (`"…"`, `'…'`), a bracketed list but a list index (`[0]` before `: `), a
/// braced object, a name in backticks after `found`, longer than [`NAME_MOST`] or not a plain
/// name, and a number, `true`, `false` or `null` where a refusal names a value: before `is not`,
/// `names no` or `does not`, or last. So `invalid input: \`name\`: ["…"] is not a String` is kept
/// as `invalid input: \`name\`: … is not a String`. The kernel quotes every map key and struct
/// member the caller sent, and the ingress every text it could not decode, so each is a quoted
/// string; a key the caller sent that no input field declares ([`NO_INPUT_FIELD`],
/// [`NO_FIELD_NAMED`]) is written `…` too.
pub(crate) fn reason(refused: &str) -> String {
    let refused = &*unknown_keys_elided(refused);
    let mut kept = String::with_capacity(REASON_MOST + ELIDED.len());
    let mut rest = refused;
    while let Some(first) = rest.chars().next() {
        if kept.len() > REASON_MOST {
            break;
        }
        let (taken, written) = match first {
            '"' => (quoted(rest, '"'), None),
            '\'' if char_literal(rest) => (quoted(rest, '\''), None),
            '{' => (balanced(rest), None),
            '[' => {
                let end = balanced(rest);
                let index = rest[end..].starts_with(": ")
                    && enclosed(&rest[..end], ']')
                        .is_some_and(|inner| inner.bytes().all(|byte| byte.is_ascii_digit()));
                (end, index.then(|| &rest[..end]))
            }
            '`' => {
                let end = quoted(rest, '`');
                let name = enclosed(&rest[..end], '`').is_some_and(|inner| {
                    inner.len() <= NAME_MOST
                        && plain(inner)
                        && scalar(inner).is_none()
                        && !kept.ends_with("found ")
                });
                (end, name.then(|| &rest[..end]))
            }
            _ if starts_word(&kept) => {
                let end = rest
                    .find(|character: char| character.is_whitespace() || ",;)".contains(character))
                    .unwrap_or(rest.len());
                let word = &rest[..end];
                let after = &rest[end..];
                let named = scalar(word).is_some()
                    && ((after.is_empty() && kept.ends_with(": "))
                        || [" is not", " names no", " does not"]
                            .iter()
                            .any(|lead| after.starts_with(lead)));
                if named {
                    (end, None)
                } else {
                    let end = first.len_utf8();
                    (end, Some(&rest[..end]))
                }
            }
            _ => {
                let end = first.len_utf8();
                (end, Some(&rest[..end]))
            }
        };
        match written {
            Some(text) => kept.push_str(text),
            None if !kept.ends_with(ELIDED) => kept.push_str(ELIDED),
            None => {}
        }
        rest = &rest[taken..];
    }
    cut(&mut kept, REASON_MOST);
    kept
}

/// The path a record keeps of the request's `path`: at most [`PATH_MOST`] bytes, cut on a
/// character boundary with `…` where longer.
fn recorded_path(path: &str) -> String {
    let mut end = path.len().min(PATH_MOST + 1);
    while !path.is_char_boundary(end) {
        end -= 1;
    }
    let mut kept = path[..end].to_owned();
    cut(&mut kept, PATH_MOST);
    kept
}

/// Cuts `text`, where longer than `most` bytes, to at most `most` on a character boundary,
/// ending `…`.
fn cut(text: &mut String, most: usize) {
    if text.len() > most {
        let mut end = most - ELIDED.len();
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str(ELIDED);
    }
}

/// `refused` with the key the caller sent written `…` where it is the ingress's refusal of a key
/// no input field declares: a JSON body's, after [`NO_INPUT_FIELD`] to the end, or a `form` or
/// `query` key, between `{at}.` and [`NO_FIELD_NAMED`]. Either may hold any text, so all of it
/// goes.
fn unknown_keys_elided(refused: &str) -> Cow<'_, str> {
    if let Some(at) = refused.find(NO_INPUT_FIELD) {
        return Cow::Owned(format!("{}{NO_INPUT_FIELD}{ELIDED}", &refused[..at]));
    }
    if let Some(place) = refused.strip_suffix(NO_FIELD_NAMED)
        && let Some((at, _)) = place.split_once('.')
    {
        return Cow::Owned(format!("{at}.{ELIDED}{NO_FIELD_NAMED}"));
    }
    Cow::Borrowed(refused)
}

/// The length of the span `text` opens with `quote`, through the next unescaped `quote`; all of
/// `text` where none closes it.
fn quoted(text: &str, quote: char) -> usize {
    let mut escaped = false;
    for (at, character) in text.char_indices().skip(1) {
        match character {
            '\\' if quote != '`' => escaped = !escaped,
            found if found == quote && !escaped => return at + found.len_utf8(),
            _ => escaped = false,
        }
    }
    text.len()
}

/// What `span` holds between its opening character and its closing `close`; `None` where it
/// holds nothing or does not close.
fn enclosed(span: &str, close: char) -> Option<&str> {
    let inner = span.get(1..)?.strip_suffix(close)?;
    (!inner.is_empty()).then_some(inner)
}

/// Whether `text` opens with a character written as Rust writes one: `'x'` or `'\n'`.
fn char_literal(text: &str) -> bool {
    let mut characters = text.chars().skip(1);
    match characters.next() {
        Some('\\') => text[1..]
            .chars()
            .take(12)
            .skip(1)
            .any(|found| found == '\''),
        Some('\'') | None => false,
        Some(_) => characters.next() == Some('\''),
    }
}

/// The length of the bracketed or braced span `text` opens with, through its closing bracket,
/// quoted strings skipped; all of `text` where it does not close.
fn balanced(text: &str) -> usize {
    let mut depth = 0usize;
    let mut at = 0;
    while let Some(character) = text[at..].chars().next() {
        match character {
            '"' => {
                at += quoted(&text[at..], '"');
                continue;
            }
            '[' | '{' => depth += 1,
            ']' | '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return at + 1;
                }
            }
            _ => {}
        }
        at += character.len_utf8();
    }
    text.len()
}

/// Whether `text` is a plain name: letters, digits, `_`, `-`, `.` and `:`.
fn plain(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_alphanumeric() || "_-.:".contains(character))
}

/// `text` as a JSON number, `true`, `false` or `null`, where it is one.
fn scalar(text: &str) -> Option<Value> {
    serde_json::from_str::<Value>(text)
        .ok()
        .filter(|value| !value.is_string() && !value.is_array() && !value.is_object())
}

/// Whether a word starts after `kept`: at the start or after a space, `:` or `(`.
fn starts_word(kept: &str) -> bool {
    kept.chars()
        .next_back()
        .is_none_or(|last| last.is_whitespace() || ":(".contains(last))
}

/// Answers a request for the reserved route `reserved` with `method`, `path` and `query` (after
/// the `?`, still percent-encoded): without a recorder every path under the prefix answers `404`;
/// with one, `GET /__spindle/requests` answers the request log, each `class=<class>` pair of the
/// query keeping only that class, and any other method `405`; `GET /__spindle/ui` answers the
/// panel's page `panel`, `503` naming it without one; `GET /__spindle/metrics` and
/// `GET /__spindle/topology` read the host's counters and kept rows, beside its thread through
/// its reader ([`read_hosted`]) or on it ([`hosted`]), so they answer `None` here, and any other
/// method `405`.
/// Every other reserved path answers `404`. A query pair other than a known `class` is refused
/// `400`.
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
        Reserved::Requests | Reserved::Metrics | Reserved::Topology if method != "GET" => {
            only_get()
        }
        Reserved::Metrics | Reserved::Topology => return None,
        Reserved::Ui => self::panel(panel, method, path).unwrap_or_else(not_found),
        Reserved::Requests | Reserved::Other => not_found(),
    })
}

/// The answer to the reserved route `reserved` on the host's thread, which [`reserved`] answered
/// `None` for: the topology for [`Reserved::Topology`], the metrics otherwise, read as
/// [`read_hosted`] reads them, the rows counted by the host where a count failed.
pub(crate) fn hosted(
    reserved: Reserved,
    recorder: &Recorder,
    host: &mut spindle_host::Host,
) -> Response {
    let started = clock();
    let reader = host.reader();
    let rows = match reserved {
        Reserved::Topology => Ok(Vec::new()),
        _ => host.rows(),
    };
    let response = answered(reserved, recorder, &reader, rows);
    timed(reserved, recorder, started);
    response
}

/// Where this build reads a clock, the listener's: the instant a scrape started. The
/// transport-free [`route`](crate::route), built without the listener for the browser, reads
/// none.
fn clock() -> Option<std::time::Instant> {
    #[cfg(feature = "listener")]
    return Some(std::time::Instant::now());
    #[cfg(not(feature = "listener"))]
    None
}

/// Notes on `recorder` how long the metrics scrape `started` took, for a metrics scrape.
fn timed(reserved: Reserved, recorder: &Recorder, started: Option<std::time::Instant>) {
    if let Some(started) = started.filter(|_| reserved != Reserved::Topology) {
        recorder.scraped(started.elapsed());
    }
}

/// The answer to the reserved route `reserved` beside the host's thread, through its `reader`,
/// which [`reserved`] answered `None` for: the topology for [`Reserved::Topology`], the metrics
/// otherwise.
pub(crate) fn read_hosted(
    reserved: Reserved,
    recorder: &Recorder,
    reader: &spindle_host::Reader,
) -> Response {
    let started = clock();
    let rows = match reserved {
        Reserved::Topology => Ok(Vec::new()),
        _ => reader.rows(),
    };
    let response = answered(reserved, recorder, reader, rows);
    timed(reserved, recorder, started);
    response
}

/// The topology for [`Reserved::Topology`]; otherwise the metrics, `rows` the rows each entity
/// holds: `200` with the text exposition ([`metrics::METRICS_MEDIA`]), `500` naming why a store
/// could not be read.
fn answered(
    reserved: Reserved,
    recorder: &Recorder,
    reader: &spindle_host::Reader,
    rows: Result<Vec<(String, usize)>, spindle_host::HostError>,
) -> Response {
    if reserved == Reserved::Topology {
        return Response {
            status: 200,
            allow: None,
            panel: false,
            metrics: false,
            body: topology::topology(recorder, reader).to_string(),
        };
    }
    match rows {
        Ok(rows) => Response {
            status: 200,
            allow: None,
            panel: false,
            metrics: true,
            body: metrics::exposition(recorder, reader, rows),
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

    /// `story:refusal-reason-bounded`: every shape in which a refusal text carries a value the
    /// caller sent (the kernel's input check, `typed.rs`, and the ingress's own refusals) is
    /// elided, the words and the names around it kept.
    #[test]
    fn a_reason_elides_every_value_and_keeps_the_names() {
        let cases = [
            (
                r#"invalid input: `name`: ["Zq"] is not a String"#,
                "invalid input: `name`: … is not a String",
            ),
            (
                "invalid input: `count`: 5 is not a String",
                "invalid input: `count`: … is not a String",
            ),
            (
                r#"invalid input: `kind`: "red" is not a variant of a.b.Colour"#,
                "invalid input: `kind`: … is not a variant of a.b.Colour",
            ),
            (
                "invalid input: `tags`: [0]: true is not a String",
                "invalid input: `tags`: [0]: … is not a String",
            ),
            (
                "invalid input: `marks`: [Zq]: 3.5 is not a Integer",
                "invalid input: `marks`: …: … is not a Integer",
            ),
            (
                r#"invalid input: `id`: "Zq" does not start with "x-", as a.b.Id does"#,
                "invalid input: `id`: … does not start with …, as a.b.Id does",
            ),
            (
                r#"invalid input: `id`: '!' in "Zq!" is not in the alphabet of a.b.Id"#,
                "invalid input: `id`: … in … is not in the alphabet of a.b.Id",
            ),
            (
                r#"invalid input: `at`: {"Zq":1} is not a a.b.Spot: its invariant `value.x > 0` is false"#,
                "invalid input: `at`: … is not a a.b.Spot: its invariant … is false",
            ),
            (
                r#"invalid input: `pick`: {"tag":"Zq"} names no variant of a.b.Pick in `tag`"#,
                "invalid input: `pick`: … names no variant of a.b.Pick in `tag`",
            ),
            (
                "query.q: expected percent-encoded UTF-8 text, found `Zq`",
                "query.q: expected percent-encoded UTF-8 text, found …",
            ),
            (
                "query: a key is not percent-encoded UTF-8: `%FFZq`",
                "query: a key is not percent-encoded UTF-8: …",
            ),
            (
                r#"body.title: expected percent-encoded UTF-8 text, found "%ZZ`Zq\"Zq""#,
                "body.title: expected percent-encoded UTF-8 text, found …",
            ),
            (
                r#"query: a key is not percent-encoded UTF-8: "%FF`Zq""#,
                "query: a key is not percent-encoded UTF-8: …",
            ),
            (
                "body.Zq: no field of this request is named so",
                "body.…: no field of this request is named so",
            ),
            (
                "query.Zq`.x: y: no field of this request is named so",
                "query.…: no field of this request is named so",
            ),
            (
                "tasks.board.OpenTask declares no input field Zq",
                "tasks.board.OpenTask declares no input field …",
            ),
            (
                "tasks.board.OpenTask declares no input field `Zq` is not [0]: x",
                "tasks.board.OpenTask declares no input field …",
            ),
            (
                r#"invalid input: `by_number`: "Zq" is not a Integer map key"#,
                "invalid input: `by_number`: … is not a Integer map key",
            ),
            (
                r#"invalid input: `by_name`: "4111111111111111": "a" is not a Integer"#,
                "invalid input: `by_name`: …: … is not a Integer",
            ),
            (
                r#"invalid input: `by_name`: "x]: [0": "a" is not a Integer"#,
                "invalid input: `by_name`: …: … is not a Integer",
            ),
            (
                "invalid input: `tags`: [4111111111111111] is not a String",
                "invalid input: `tags`: … is not a String",
            ),
            (
                r#"invalid input: `at`: a.b.Spot declares no member "Zq""#,
                "invalid input: `at`: a.b.Spot declares no member …",
            ),
            (
                "the body is not a JSON object: 42",
                "the body is not a JSON object: …",
            ),
            (
                r#"the body is not a JSON object: ["Zq"]"#,
                "the body is not a JSON object: …",
            ),
            (
                "the caller's `owner`: invalid type: integer `7`, expected a string",
                "the caller's `owner`: invalid type: integer …, expected a string",
            ),
            (
                r#"binding thank-on-close: the input reader_id is unmet: "member-0042" is not a library.notices.ReaderRef: its invariant `value.count <= 8` is false"#,
                "binding thank-on-close: the input reader_id is unmet: … is not a \
                 library.notices.ReaderRef: its invariant … is false",
            ),
        ];
        for (refused, kept) in cases {
            assert_eq!(super::reason(refused), kept, "{refused}");
        }
        for unchanged in [
            "the body is not JSON: expected value at line 1 column 5",
            "invalid input: `tags`: [0]: [1]: … is not a String",
            "query.page: expected one value, found 2",
            "neither the request nor a decisions file answers `external.vetoed`, and \
             `tasks.ledger.Review` declares no default branch for --answer-default to take",
            "the command reads the caller attribute `owner_id`, and it is sent as no caller \
             that carries it",
            "held back by the test",
        ] {
            assert_eq!(super::reason(unchanged), unchanged);
        }
    }

    #[test]
    fn a_path_is_cut_to_its_bound_on_a_character_boundary() {
        for long in ["/p".repeat(40_000), format!("/{}", "é".repeat(30_000))] {
            let kept = super::recorded_path(&long);
            assert!(kept.len() <= super::PATH_MOST, "{} bytes", kept.len());
            assert!(kept.len() > super::PATH_MOST - 8, "{} bytes", kept.len());
            assert!(kept.ends_with('…'), "{kept}");
            assert!(long.starts_with(kept.trim_end_matches('…')));
        }
        let exact = format!("/{}", "p".repeat(super::PATH_MOST - 1));
        assert_eq!(super::recorded_path(&exact), exact);
        assert_eq!(super::recorded_path("/a/b"), "/a/b");
    }

    #[test]
    fn a_reason_is_cut_to_its_bound() {
        for long in [
            "x".repeat(1_000),
            "é".repeat(500),
            format!("a {}", "`b` ".repeat(300)),
        ] {
            let kept = super::reason(&long);
            assert!(kept.len() <= super::REASON_MOST, "{} bytes", kept.len());
            assert!(kept.ends_with('…'), "{kept}");
        }
        let sent = format!(
            r#"invalid input: `name`: ["{}"] is not a String"#,
            "Zq".repeat(400_000)
        );
        assert_eq!(
            super::reason(&sent),
            "invalid input: `name`: … is not a String"
        );
    }

    #[test]
    fn a_recorder_holds_its_capacity_within_bounds() {
        assert_eq!(Recorder::new(0).capacity(), 1);
        assert_eq!(Recorder::new(20_000).capacity(), super::REQUESTS_MOST);
    }

    /// A timer-fired command asks its causes while no request is served: each is counted, and
    /// none is kept, however many the timer fires before the next request.
    #[test]
    fn causes_asked_outside_a_request_are_counted_and_not_kept() {
        let recorder = Recorder::new(4);
        let asked = || super::ExternalAnswer {
            command: "tick.clock.Fire".to_owned(),
            outcome: "skipped".to_owned(),
            holds: Some(false),
            source: super::AnswerSource::File,
        };
        for _ in 0..1000 {
            recorder.answered(asked());
        }
        assert_eq!(recorder.ring(|ring| ring.answers.len()), 0);
        assert_eq!(
            recorder.traffic().externals,
            vec![(
                [
                    "tick.clock.Fire".to_owned(),
                    "skipped".to_owned(),
                    "file".to_owned()
                ],
                1000
            )]
        );
        recorder.serving();
        recorder.answered(asked());
        assert_eq!(recorder.served(), vec![asked()]);
        recorder.answered(asked());
        assert_eq!(recorder.ring(|ring| ring.answers.len()), 0);
    }
}

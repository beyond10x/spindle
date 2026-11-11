//! The routes a generic model console reads, served beside the routes ESS publishes where
//! `ingress.network` is bound to `http-loopback`: the model, the events published so far, and a
//! reset of every store, which may load a world.

use ess_compiler::EssIr;
use ess_http::{Method, Operation};
use serde_json::{Value, json};
use spindle_host::{Cursor, Host, HostError, Page, Published, Reader, World};

use crate::answer::Answer;
use crate::surface::{Surface, form_decoded};

/// One console route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Console {
    /// `GET /api/model`: the compiled IR and each outcome's subject.
    Model,
    /// `GET /api/events`: every event published so far, in order, or a range of them after a
    /// cursor ([`events_queried`]).
    Events,
    /// `POST /api/reset`: empty every store of the run, then load the world its body carries.
    Reset,
    /// `GET /api/served`: every command and view the surface routes, whether it serves it, and
    /// why not ([`served`]).
    Served,
}

impl Console {
    /// Every console route.
    pub const ALL: [Self; 4] = [Self::Model, Self::Events, Self::Reset, Self::Served];

    /// The method it answers.
    pub fn method(self) -> Method {
        match self {
            Self::Model | Self::Events | Self::Served => Method::Get,
            Self::Reset => Method::Post,
        }
    }

    /// Its path.
    pub fn path(self) -> &'static str {
        match self {
            Self::Model => "/api/model",
            Self::Events => "/api/events",
            Self::Reset => "/api/reset",
            Self::Served => "/api/served",
        }
    }
}

impl std::fmt::Display for Console {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} {}",
            self.method().as_str().to_uppercase(),
            self.path()
        )
    }
}

/// The body `GET /api/model` answers, as text: `{"ir": <IR>, "subjects": [...]}`.
///
/// `ir` is written byte for byte as `ess specify compile --format json` prints it
/// ([`EssIr::to_canonical_json`]). `subjects` holds one entry per outcome of every command, in
/// the IR's order: `{"command", "outcome", "subject"}`, `subject` the entity the outcome acts on,
/// what it does to it and where the instance's identity is read, as the IR carries it, or `null`
/// for an outcome that changes no entity.
pub fn model(ir: &EssIr) -> String {
    let subjects = ir
        .commands()
        .iter()
        .flat_map(|(name, command)| {
            command.outcomes.iter().map(move |outcome| {
                json!({
                    "command": name.to_string(),
                    "outcome": outcome.name.to_string(),
                    "subject": serde_json::to_value(&outcome.subject).unwrap_or(Value::Null),
                })
            })
        })
        .collect::<Vec<_>>();
    format!(
        "{{\"ir\":{},\"subjects\":{}}}",
        ir.to_canonical_json(),
        Value::Array(subjects)
    )
}

/// What `GET /api/events` answers: `200` with `{"events": [...]}`, each event the host published
/// as `{"component", "command", "event", "payload"}`, in the order published, read from the
/// stores that committed them ([`Host::published`]); `500` naming why where a store cannot be
/// read.
pub fn events(host: &mut Host) -> Answer {
    answered(host.published())
}

/// What `GET /api/events` answers, read beside the host's thread through its `reader`, as
/// [`events`] answers it ([`Reader::published`]).
pub fn events_read(reader: &Reader) -> Answer {
    answered(reader.published())
}

/// How many events a range of `GET /api/events` answers where its query names no `limit`.
pub const EVENTS_LIMIT_DEFAULT: usize = 100;

/// The most events a range of `GET /api/events` answers: a larger `limit` is answered with it.
pub const EVENTS_LIMIT_MOST: usize = 1_000;

/// The query parameter a range of `GET /api/events` starts after.
pub(crate) const AFTER: &str = "after";

/// The query parameter that bounds a range of `GET /api/events`.
pub(crate) const LIMIT: &str = "limit";

/// What `GET /api/events?<query>` answers on the host's thread. Where `query` (after the `?`,
/// still percent-encoded) names neither `after` nor `limit`, [`events`], as without a query; a
/// parameter it does not read is ignored, as a view's query is read. Otherwise `200` with
/// `{"events": [...], "next": <cursor>}`: the events published after the cursor `after` (from
/// the first without one), at most `limit` ([`EVENTS_LIMIT_DEFAULT`] without one, and at most
/// [`EVENTS_LIMIT_MOST`]: a larger one is answered with it), in the order [`events`] lists
/// them, and the cursor after the last of them ([`Host::published_after`]); with the cursor at
/// the end, no event and the same cursor. A malformed `after` or `limit`, either given twice, a
/// `limit` of 0 and a cursor of another run's components are refused `400` naming the
/// parameter, never its value; a store that cannot be read `500`.
pub fn events_queried(host: &mut Host, query: &str) -> Answer {
    match ranged(query) {
        Ok(None) => events(host),
        Ok(Some((after, limit))) => paged(host.published_after(&after, limit)),
        Err(why) => Answer::refusal(400, why),
    }
}

/// What `GET /api/events?<query>` answers, read beside the host's thread through its `reader`,
/// as [`events_queried`] answers it ([`Reader::published_after`]).
pub fn events_read_queried(reader: &Reader, query: &str) -> Answer {
    match ranged(query) {
        Ok(None) => events_read(reader),
        Ok(Some((after, limit))) => paged(reader.published_after(&after, limit)),
        Err(why) => Answer::refusal(400, why),
    }
}

/// The range `query` asks for, a cursor and a limit; `None` where it names neither parameter;
/// the refusal of a malformed one, which never repeats its value.
fn ranged(query: &str) -> Result<Option<(Cursor, usize)>, String> {
    let mut after = Vec::new();
    let mut limit = Vec::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match form_decoded(key).as_deref() {
            Some(AFTER) => after.push(value),
            Some(LIMIT) => limit.push(value),
            _ => {}
        }
    }
    if after.is_empty() && limit.is_empty() {
        return Ok(None);
    }
    let after = match one(AFTER, &after)? {
        None => Cursor::start(),
        Some(raw) => form_decoded(raw)
            .ok_or_else(|| "expected percent-encoded UTF-8 text".to_owned())
            .and_then(|text| Cursor::parse(&text))
            .map_err(|why| format!("query.{AFTER}: {why}"))?,
    };
    let limit = match one(LIMIT, &limit)? {
        None => EVENTS_LIMIT_DEFAULT,
        Some(raw) => {
            let malformed = || format!("query.{LIMIT}: expected a whole number of events, from 1");
            let text = form_decoded(raw).ok_or_else(malformed)?;
            if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(malformed());
            }
            // Only digits: a number too large for any integer is larger than the maximum.
            match text.parse::<usize>() {
                Ok(0) => return Err(malformed()),
                Ok(limit) => limit.min(EVENTS_LIMIT_MOST),
                Err(_) => EVENTS_LIMIT_MOST,
            }
        }
    };
    Ok(Some((after, limit)))
}

/// The one value `values` holds for the parameter `name`, if any; the refusal of several.
fn one<'a>(name: &str, values: &[&'a str]) -> Result<Option<&'a str>, String> {
    match values {
        [] => Ok(None),
        [value] => Ok(Some(value)),
        several => Err(format!(
            "query.{name}: expected one value, found {}",
            several.len()
        )),
    }
}

/// `200` with `{"events": [...], "next": <cursor>}` for `page`; `400` for a cursor of another
/// run's components, `500` naming why a store could not be read.
fn paged(page: Result<Page, HostError>) -> Answer {
    match page {
        Ok(page) => {
            let mut answer = listed(&page.events);
            if let Some(Value::Object(body)) = &mut answer.body {
                body.insert("next".to_owned(), Value::String(page.next.to_string()));
            }
            answer
        }
        Err(error @ HostError::Cursor(_)) => {
            Answer::refusal(400, format!("query.{AFTER}: {error}"))
        }
        Err(error) => Answer::refusal(500, error.to_string()),
    }
}

fn answered(published: Result<Vec<Published>, HostError>) -> Answer {
    match published {
        Ok(published) => listed(&published),
        Err(error) => Answer::refusal(500, error.to_string()),
    }
}

/// `200` with `{"events": [...]}`, each of `published` in order.
fn listed(published: &[Published]) -> Answer {
    let events = published
        .iter()
        .map(|published| {
            json!({
                "component": published.component,
                "command": published.command,
                "event": published.event.name,
                "payload": published.event.payload,
            })
        })
        .collect::<Vec<_>>();
    Answer {
        status: 200,
        body: Some(json!({"events": events})),
    }
}

/// The body `GET /api/served` answers, `spindle.observe.ServedOperations`:
/// `{"operations": [...]}`, one row per route of `surface` in the order its start log names them
/// ([`Surface::operations`]), each `{"component", "name", "kind", "method", "path", "served"}`,
/// `kind` `command` or `view`, and for one not served `reason`, the text [`Route::not_served`]
/// gives it ([`Route::unserved`]). The surface computes it once, when the console routes are
/// added ([`Surface::with_console`]).
///
/// [`Route::not_served`]: crate::Route::not_served
/// [`Route::unserved`]: crate::Route::unserved
pub fn served(surface: &Surface) -> Value {
    let operations = surface
        .operations()
        .into_iter()
        .map(|route| {
            let mut row = json!({
                "component": route.component,
                "name": route.operation.name().to_string(),
                "kind": match route.operation {
                    Operation::Command(_) => "command",
                    Operation::View(_) => "view",
                },
                "method": route.method.as_str().to_uppercase(),
                "path": route.path,
                "served": route.unserved().is_none(),
            });
            if let (Some(why), Value::Object(row)) = (route.unserved(), &mut row) {
                row.insert("reason".to_owned(), Value::String(why.to_string()));
            }
            row
        })
        .collect::<Vec<_>>();
    json!({"operations": operations})
}

/// Reads the world a `POST /api/reset` body carries, as JSON, against the model the host runs;
/// the reason it is refused otherwise.
pub type Worlds<'a> = &'a dyn Fn(&EssIr, &Value) -> Result<World, String>;

/// What `POST /api/reset` answers once every store of the run is empty and the world `body`
/// carries is loaded ([`Host::reset_to`]): `200` with `{"emptied": [<component>, ...]}`.
///
/// A body empty or all whitespace loads no world. Any other body is read as JSON by `worlds`; a
/// body that is not JSON, a run with no `worlds`, and a world `worlds` or the host refuses are
/// refused `400` before any store is emptied. A store that could not be emptied or written is
/// refused `500`, naming why.
pub fn reset(host: &mut Host, body: &[u8], worlds: Option<Worlds<'_>>) -> Answer {
    let world = if body.iter().all(u8::is_ascii_whitespace) {
        World::default()
    } else {
        let read = serde_json::from_slice::<Value>(body)
            .map_err(|error| format!("the reset body is not JSON: {error}"))
            .and_then(|value| match worlds {
                Some(worlds) => worlds(host.ir(), &value),
                None => Err("this run loads no world; send the reset without a body".to_owned()),
            });
        match read {
            Ok(world) => world,
            Err(why) => return Answer::refusal(400, why),
        }
    };
    match host.reset_to(world) {
        Ok(emptied) => Answer {
            status: 200,
            body: Some(json!({"emptied": emptied})),
        },
        Err(error @ HostError::World(_)) => Answer::refusal(400, error.to_string()),
        Err(error) => Answer::refusal(500, error.to_string()),
    }
}
